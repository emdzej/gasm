/*
  SDL threads on gasm: cooperative threads from the C SDK (gasm_thread.h,
  design/threads.md), in place of SDL's generic stubs (src/thread/generic is left
  out of the build: SDL_systhread.c, SDL_sysmutex.c, SDL_syscond.c, SDL_syssem.c,
  SDL_systls.c, SDL_sysrwlock.c). SDL_THREADS_DISABLED stays defined (it selects
  SDL's generic thread handle); with it SDL keeps one error buffer for all threads
  and doesn't lock its event queue, which is safe here: threads only switch where
  one blocks.

  Threads run on the guest's one wasm thread, one at a time, switching when one
  blocks, yields or waits for the next frame; SDL_CreateThread works in apps
  linked with the threaded loop helper (classic main(), gasm_loop_threads.o,
  CMake: gasm_sdl3_app(<target> LOOP THREADS)). Elsewhere it fails with an
  error, as before, and mutexes, conditions and semaphores work as on one thread.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"

#include "thread/SDL_systhread.h"
#include "thread/SDL_thread_c.h"

#include "gasm.h"
#include "gasm_thread.h"

/* ---- threads ----------------------------------------------------------------------- */

/* SDL's thread handle is an int (generic SYS_ThreadHandle): an index here */
static gasm_thread **handles;
static int nhandles;

static int SDLCALL RunThread(void *data)
{
    SDL_RunThread((SDL_Thread *)data);
    return 0;
}

bool SDL_SYS_CreateThread(SDL_Thread *thread, SDL_FunctionPointer pfnBeginThread, SDL_FunctionPointer pfnEndThread)
{
    (void)pfnBeginThread;
    (void)pfnEndThread;
    gasm_thread *t = gasm_thread_create(RunThread, thread, thread->stacksize);
    if (!t) {
        return SDL_SetError("SDL threads on gasm need a classic main() linked with the threaded loop helper "
                            "(gasm_sdl3_app(<target> LOOP THREADS))");
    }
    int h;
    for (h = 0; h < nhandles && handles[h]; h++) {
    }
    if (h == nhandles) {
        gasm_thread **grown = (gasm_thread **)SDL_realloc(handles, sizeof *handles * (nhandles + 8));
        if (!grown) {
            gasm_thread_detach(t);
            return SDL_OutOfMemory();
        }
        SDL_memset(grown + nhandles, 0, sizeof *handles * 8);
        handles = grown;
        nhandles += 8;
    }
    handles[h] = t;
    thread->handle = h;
    return true;
}

void SDL_SYS_SetupThread(const char *name)
{
    (void)name;
}

SDL_ThreadID SDL_GetCurrentThreadID(void)
{
    return (SDL_ThreadID)gasm_thread_id(gasm_thread_self()) + 1;   /* never 0 */
}

bool SDL_SYS_SetThreadPriority(SDL_ThreadPriority priority)
{
    (void)priority;
    return true;
}

static gasm_thread *Take(SDL_Thread *thread)
{
    int h = thread->handle;
    if (h < 0 || h >= nhandles) {
        return NULL;
    }
    gasm_thread *t = handles[h];
    handles[h] = NULL;
    return t;
}

void SDL_SYS_WaitThread(SDL_Thread *thread)
{
    gasm_thread *t = Take(thread);
    if (t) {
        gasm_thread_join(t);
    }
}

void SDL_SYS_DetachThread(SDL_Thread *thread)
{
    gasm_thread *t = Take(thread);
    if (t) {
        gasm_thread_detach(t);
    }
}

/* ---- thread-local storage ------------------------------------------------------------ */

static int tls_key = -1;

void SDL_SYS_InitTLSData(void)
{
    if (tls_key < 0) {
        tls_key = gasm_thread_key_create(NULL);
    }
}

SDL_TLSData *SDL_SYS_GetTLSData(void)
{
    return tls_key < 0 ? NULL : (SDL_TLSData *)gasm_thread_get(tls_key);
}

bool SDL_SYS_SetTLSData(SDL_TLSData *data)
{
    if (tls_key < 0) {
        return SDL_SetError("TLS not initialized");
    }
    gasm_thread_set(tls_key, data);
    return true;
}

void SDL_SYS_QuitTLSData(void)
{
    if (tls_key >= 0) {
        gasm_thread_key_delete(tls_key);
        tls_key = -1;
    }
}

/* ---- mutexes (recursive, as SDL's are) ----------------------------------------------- */

struct SDL_Mutex
{
    gasm_mutex m;
};

SDL_Mutex *SDL_CreateMutex(void)
{
    SDL_Mutex *mutex = (SDL_Mutex *)SDL_calloc(1, sizeof *mutex);
    if (mutex) {
        mutex->m.recursive = 1;
    }
    return mutex;
}

void SDL_DestroyMutex(SDL_Mutex *mutex)
{
    SDL_free(mutex);
}

void SDL_LockMutex(SDL_Mutex *mutex) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (mutex) {
        gasm_mutex_lock(&mutex->m);
    }
}

bool SDL_TryLockMutex(SDL_Mutex *mutex) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    return mutex ? gasm_mutex_trylock(&mutex->m) != 0 : true;
}

void SDL_UnlockMutex(SDL_Mutex *mutex) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (mutex) {
        gasm_mutex_unlock(&mutex->m);
    }
}

/* ---- conditions and semaphores (timeouts in gasm_time_ms, the frame's time) ------------ */

static double Deadline(Sint64 timeoutNS)
{
    return timeoutNS < 0 ? GASM_FOREVER : gasm_time_ms() + (double)timeoutNS / 1e6;
}

struct SDL_Condition
{
    gasm_cond c;
};

SDL_Condition *SDL_CreateCondition(void)
{
    return (SDL_Condition *)SDL_calloc(1, sizeof(SDL_Condition));
}

void SDL_DestroyCondition(SDL_Condition *cond)
{
    SDL_free(cond);
}

void SDL_SignalCondition(SDL_Condition *cond)
{
    if (cond) {
        gasm_cond_signal(&cond->c);
    }
}

void SDL_BroadcastCondition(SDL_Condition *cond)
{
    if (cond) {
        gasm_cond_broadcast(&cond->c);
    }
}

bool SDL_WaitConditionTimeoutNS(SDL_Condition *cond, SDL_Mutex *mutex, Sint64 timeoutNS)
{
    if (!cond || !mutex) {
        return true;
    }
    return gasm_cond_wait(&cond->c, &mutex->m, Deadline(timeoutNS)) == 0;
}

/* ---- read/write locks ------------------------------------------------------------------ */

struct SDL_RWLock
{
    gasm_mutex m;
    gasm_cond c;
    int readers;
    gasm_thread *writer;
    int writes;          /* the writer's recursion */
};

SDL_RWLock *SDL_CreateRWLock(void)
{
    return (SDL_RWLock *)SDL_calloc(1, sizeof(SDL_RWLock));
}

void SDL_DestroyRWLock(SDL_RWLock *rwlock)
{
    SDL_free(rwlock);
}

void SDL_LockRWLockForReading(SDL_RWLock *rwlock) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (!rwlock) {
        return;
    }
    gasm_mutex_lock(&rwlock->m);
    while (rwlock->writer && rwlock->writer != gasm_thread_self()) {
        gasm_cond_wait(&rwlock->c, &rwlock->m, GASM_FOREVER);
    }
    rwlock->readers++;
    gasm_mutex_unlock(&rwlock->m);
}

void SDL_LockRWLockForWriting(SDL_RWLock *rwlock) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (!rwlock) {
        return;
    }
    gasm_thread *self = gasm_thread_self();
    gasm_mutex_lock(&rwlock->m);
    if (rwlock->writer == self) {
        rwlock->writes++;
    } else {
        while (rwlock->writer || rwlock->readers > 0) {
            gasm_cond_wait(&rwlock->c, &rwlock->m, GASM_FOREVER);
        }
        rwlock->writer = self;
        rwlock->writes = 1;
    }
    gasm_mutex_unlock(&rwlock->m);
}

bool SDL_TryLockRWLockForReading(SDL_RWLock *rwlock) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (!rwlock) {
        return true;
    }
    if (rwlock->writer && rwlock->writer != gasm_thread_self()) {
        return false;
    }
    rwlock->readers++;
    return true;
}

bool SDL_TryLockRWLockForWriting(SDL_RWLock *rwlock) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (!rwlock) {
        return true;
    }
    gasm_thread *self = gasm_thread_self();
    if (rwlock->writer == self) {
        rwlock->writes++;
        return true;
    }
    if (rwlock->writer || rwlock->readers > 0) {
        return false;
    }
    rwlock->writer = self;
    rwlock->writes = 1;
    return true;
}

void SDL_UnlockRWLock(SDL_RWLock *rwlock) SDL_NO_THREAD_SAFETY_ANALYSIS
{
    if (!rwlock) {
        return;
    }
    gasm_mutex_lock(&rwlock->m);
    if (rwlock->writer == gasm_thread_self() && rwlock->writes > 0) {
        if (--rwlock->writes == 0) {
            rwlock->writer = NULL;
        }
    } else if (rwlock->readers > 0) {
        rwlock->readers--;
    }
    gasm_cond_broadcast(&rwlock->c);
    gasm_mutex_unlock(&rwlock->m);
}

struct SDL_Semaphore
{
    gasm_sem s;
};

SDL_Semaphore *SDL_CreateSemaphore(Uint32 initial_value)
{
    SDL_Semaphore *sem = (SDL_Semaphore *)SDL_calloc(1, sizeof *sem);
    if (sem) {
        sem->s.count = initial_value;
    }
    return sem;
}

void SDL_DestroySemaphore(SDL_Semaphore *sem)
{
    SDL_free(sem);
}

bool SDL_WaitSemaphoreTimeoutNS(SDL_Semaphore *sem, Sint64 timeoutNS)
{
    if (!sem) {
        return true;
    }
    if (timeoutNS == 0) {
        return gasm_sem_trywait(&sem->s) != 0;
    }
    return gasm_sem_wait(&sem->s, Deadline(timeoutNS)) == 0;
}

Uint32 SDL_GetSemaphoreValue(SDL_Semaphore *sem)
{
    return sem ? sem->s.count : 0;
}

void SDL_SignalSemaphore(SDL_Semaphore *sem)
{
    if (sem) {
        gasm_sem_post(&sem->s);
    }
}
