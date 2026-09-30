/*
 * OPL (AdLib / Sound Blaster FM) backend for chocolate-doom's music player
 * (i_oplmusic.c), on top of the DOSBox OPL emulator (dbopl.c).
 *
 * Desktop ports run the emulator on an audio thread. Here everything is
 * driven from gasm_frame: gasm_opl_render() generates a frame's worth of
 * samples and fires the player's timer callbacks at their exact sample
 * positions. Music time is therefore counted in samples, not wall-clock
 * time, which keeps it identical on every runner.
 *
 * SPDX-License-Identifier: MIT
 */
#include <stdint.h>
#include <string.h>

#include "opl.h"
#include "opl_queue.h"
#include "dbopl.h"
#include "gasm_doom.h"

#define MAX_BLOCK 2048

static Chip chip;
static opl_callback_queue_t *queue;
static unsigned int rate = 44100;
static uint64_t samples_done;   /* since OPL_Init */
static uint64_t now_us;         /* samples_done in microseconds */
static uint64_t pause_offset;   /* time spent paused, excluded from callbacks */
static int paused;
static int opl3;
static int initialized;
static unsigned int reg_num;
static int32_t block[MAX_BLOCK * 2];

void OPL_SetSampleRate(unsigned int r) { rate = r; }

opl_init_result_t OPL_Init(unsigned int port_base) {
    (void)port_base;
    if (!initialized) {
        queue = OPL_Queue_Create();
        DBOPL_InitTables();
        initialized = 1;
    }
    Chip__Chip(&chip);
    Chip__Setup(&chip, rate);
    OPL_Queue_Clear(queue);
    samples_done = now_us = pause_offset = 0;
    paused = opl3 = 0;
    return OPL_INIT_OPL3;
}

void OPL_Shutdown(void) {
    if (initialized) OPL_Queue_Clear(queue);
}

opl_init_result_t OPL_Detect(void) { return initialized ? OPL_INIT_OPL3 : OPL_INIT_NONE; }

/* No hardware timers: the player schedules everything with OPL_SetCallback. */
unsigned int OPL_ReadPort(opl_port_t port) { return port == OPL_REGISTER_PORT_OPL3 ? 0xff : 0; }
unsigned int OPL_ReadStatus(void) { return 0; }

void OPL_WriteRegister(int reg, int value) {
    if (reg == OPL_REG_TIMER1 || reg == OPL_REG_TIMER2 || reg == OPL_REG_TIMER_CTRL) return;
    if (reg == OPL_REG_NEW) opl3 = value & 1;
    Chip__WriteReg(&chip, (Bit32u)reg, (Bit8u)value);
}

void OPL_WritePort(opl_port_t port, unsigned int value) {
    if (port == OPL_REGISTER_PORT) reg_num = value;
    else if (port == OPL_REGISTER_PORT_OPL3) reg_num = value | 0x100;
    else OPL_WriteRegister((int)reg_num, (int)value);
}

/* The register reset sequence DOOM's sound library performs at startup. */
static void init_bank(int bank) {
    for (int r = OPL_REGS_LEVEL; r <= OPL_REGS_LEVEL + OPL_NUM_OPERATORS; ++r)
        OPL_WriteRegister(r | bank, 0x3f);
    for (int r = OPL_REGS_ATTACK; r <= OPL_REGS_WAVEFORM + OPL_NUM_OPERATORS; ++r)
        OPL_WriteRegister(r | bank, 0x00);
    for (int r = 1; r < OPL_REGS_LEVEL; ++r)
        OPL_WriteRegister(r | bank, 0x00);
}

void OPL_InitRegisters(int want_opl3) {
    init_bank(0);
    OPL_WriteRegister(OPL_REG_WAVEFORM_ENABLE, 0x20);
    if (want_opl3) {
        OPL_WriteRegister(OPL_REG_NEW, 0x01);
        init_bank(0x100);
    }
    OPL_WriteRegister(OPL_REG_FM_MODE, 0x40);
    if (want_opl3) OPL_WriteRegister(OPL_REG_NEW, 0x01);
}

void OPL_SetCallback(uint64_t us, opl_callback_t callback, void *data) {
    OPL_Queue_Push(queue, callback, data, now_us - pause_offset + us);
}

void OPL_ClearCallbacks(void) { OPL_Queue_Clear(queue); }
void OPL_AdjustCallbacks(float factor) { OPL_Queue_AdjustCallbacks(queue, now_us, factor); }
void OPL_Lock(void) {}
void OPL_Unlock(void) {}
void OPL_SetPaused(int p) { paused = p; }

/* Nothing runs concurrently, so a delay can only move music time forward. */
void OPL_Delay(uint64_t us) {
    now_us += us;
    samples_done = now_us * rate / OPL_SECOND;
}

static void run_due_callbacks(void) {
    opl_callback_t callback;
    void *data;
    while (!paused && !OPL_Queue_IsEmpty(queue) && now_us >= OPL_Queue_Peek(queue) + pause_offset) {
        if (!OPL_Queue_Pop(queue, &callback, &data)) break;
        callback(data);
    }
}

static void advance(unsigned int n) {
    uint64_t before = now_us;
    samples_done += n;
    now_us = samples_done * OPL_SECOND / rate;
    if (paused) pause_offset += now_us - before;
}

void gasm_opl_render(int32_t *stereo, unsigned int frames) {
    if (!initialized) {
        memset(stereo, 0, sizeof(int32_t) * 2 * frames);
        return;
    }
    unsigned int done = 0;
    while (done < frames) {
        run_due_callbacks();
        unsigned int n = frames - done;
        if (n > MAX_BLOCK) n = MAX_BLOCK;
        if (!paused && !OPL_Queue_IsEmpty(queue)) {
            /* stop exactly at the next callback so note timing is sample-accurate */
            uint64_t next = OPL_Queue_Peek(queue) + pause_offset;
            uint64_t k = next > now_us ? ((next - now_us) * rate + OPL_SECOND - 1) / OPL_SECOND : 1;
            if (k < n) n = (unsigned int)k;
        }
        if (opl3) {
            Chip__GenerateBlock3(&chip, n, block);
            memcpy(stereo + 2 * done, block, sizeof(int32_t) * 2 * n);
        } else {
            Chip__GenerateBlock2(&chip, n, block);
            for (unsigned int i = 0; i < n; i++) stereo[2 * (done + i)] = stereo[2 * (done + i) + 1] = block[i];
        }
        done += n;
        advance(n);
    }
    run_due_callbacks();
}
