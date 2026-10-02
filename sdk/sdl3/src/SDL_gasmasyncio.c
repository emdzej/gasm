/*
  SDL3 on gasm: async I/O, done synchronously (gasm guests have one thread).
  Replaces SDL's generic backend, which needs a thread pool: each task runs when
  it is queued and its result waits in the queue, so SDL_GetAsyncIOResult and
  SDL_WaitAsyncIOResult return it on the next call. Adapted from
  src/io/generic/SDL_asyncio_generic.c.

  SPDX-License-Identifier: Zlib
*/
#include "SDL_internal.h"
#include "io/SDL_sysasyncio.h"

typedef struct
{
    SDL_AsyncIOTask completed_tasks;
} QueueData;

static Sint64 GASM_asyncio_size(void *userdata)
{
    return SDL_GetIOSize((SDL_IOStream *)userdata);
}

static void Complete(SDL_AsyncIOTask *task)
{
    QueueData *data = (QueueData *)task->queue->userdata;
    LINKED_LIST_PREPEND(task, data->completed_tasks, queue);
}

static void RunTask(SDL_AsyncIOTask *task)
{
    SDL_IOStream *io = (SDL_IOStream *)task->asyncio->userdata;
    if (task->type == SDL_ASYNCIO_TASK_CLOSE) {
        bool okay = true;
        if (task->flush) {
            okay = SDL_FlushIO(io);
        }
        okay = SDL_CloseIO(io) && okay;
        task->result = okay ? SDL_ASYNCIO_COMPLETE : SDL_ASYNCIO_FAILURE;
    } else if (SDL_SeekIO(io, (Sint64)task->offset, SDL_IO_SEEK_SET) < 0) {
        task->result = SDL_ASYNCIO_FAILURE;
    } else {
        const bool writing = task->type == SDL_ASYNCIO_TASK_WRITE;
        const size_t size = (size_t)task->requested_size;
        task->result_size = (Uint64)(writing ? SDL_WriteIO(io, task->buffer, size) : SDL_ReadIO(io, task->buffer, size));
        if (task->result_size == task->requested_size) {
            task->result = SDL_ASYNCIO_COMPLETE;
        } else if (writing) {
            task->result = SDL_ASYNCIO_FAILURE;
        } else {
            task->result = SDL_GetIOStatus(io) == SDL_IO_STATUS_EOF ? SDL_ASYNCIO_COMPLETE : SDL_ASYNCIO_FAILURE;
        }
    }
    Complete(task);
}

static bool GASM_asyncio_io(void *userdata, SDL_AsyncIOTask *task)
{
    (void)userdata;
    return task->queue->iface.queue_task(task->queue->userdata, task);
}

static void GASM_asyncio_destroy(void *userdata)
{
    (void)userdata;   /* the stream was closed by the close task */
}

static bool GASM_queue_task(void *userdata, SDL_AsyncIOTask *task)
{
    (void)userdata;
    RunTask(task);
    return true;
}

static void GASM_cancel_task(void *userdata, SDL_AsyncIOTask *task)
{
    (void)userdata;
    task->result = SDL_ASYNCIO_CANCELED;
    Complete(task);
}

static SDL_AsyncIOTask *GASM_get_results(void *userdata)
{
    QueueData *data = (QueueData *)userdata;
    SDL_AsyncIOTask *task = LINKED_LIST_START(data->completed_tasks, queue);
    if (task) {
        LINKED_LIST_UNLINK(task, queue);
    }
    return task;
}

/* Everything queued has completed already: waiting can't produce more. */
static SDL_AsyncIOTask *GASM_wait_results(void *userdata, Sint32 timeoutMS)
{
    (void)timeoutMS;
    return GASM_get_results(userdata);
}

static void GASM_signal(void *userdata)
{
    (void)userdata;
}

static void GASM_destroy(void *userdata)
{
    SDL_free(userdata);
}

bool SDL_SYS_CreateAsyncIOQueue(SDL_AsyncIOQueue *queue)
{
    QueueData *data = (QueueData *)SDL_calloc(1, sizeof(*data));
    if (!data) {
        return false;
    }
    static const SDL_AsyncIOQueueInterface iface = {
        GASM_queue_task, GASM_cancel_task, GASM_get_results, GASM_wait_results, GASM_signal, GASM_destroy
    };
    SDL_copyp(&queue->iface, &iface);
    queue->userdata = data;
    return true;
}

bool SDL_SYS_AsyncIOFromFile(const char *file, const char *mode, SDL_AsyncIO *asyncio)
{
    SDL_IOStream *io = SDL_IOFromFile(file, mode);
    if (!io) {
        return false;
    }
    static const SDL_AsyncIOInterface iface = {
        GASM_asyncio_size, GASM_asyncio_io, GASM_asyncio_io, GASM_asyncio_io, GASM_asyncio_destroy
    };
    SDL_copyp(&asyncio->iface, &iface);
    asyncio->userdata = io;
    return true;
}

void SDL_SYS_QuitAsyncIO(void)
{
}
