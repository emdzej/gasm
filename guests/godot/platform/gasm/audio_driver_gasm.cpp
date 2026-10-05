/**************************************************************************/
/*  audio_driver_gasm.cpp                                                 */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#include "audio_driver_gasm.h"

#include "core/config/project_settings.h"

#include "gasm.h"

Error AudioDriverGasm::init() {
	mix_rate = _get_configured_mix_rate();
	if (mix_rate <= 0) {
		mix_rate = 44100;
	}
	gasm_audio_config((uint32_t)mix_rate, 2);
	return OK;
}

void AudioDriverGasm::mix_frame(double p_frame_rate) {
	if (!active) {
		return;
	}
	// up to the end of this frame, by the frame's time: natively real time, so a slow frame
	// (the game below 60 fps, the runner's catch-up) mixes more and the sound doesn't run
	// dry; in headless runs virtual time, exactly rate / frame_rate frames each (735 at
	// 44.1 kHz and 60 Hz: the same audio as mixing per frame)
	const double now = gasm_time_ms();
	if (start_ms < 0.0) {
		start_ms = now;
	}
	uint64_t want = (uint64_t)((now - start_ms) * (double)mix_rate / 1000.0 + (double)mix_rate / p_frame_rate + 0.5);
	// a long stall (a suspended app, a debugger): play on from now, not all of it at once
	if (want > frames_out + (uint64_t)mix_rate / 4) {
		frames_out = want - (uint64_t)mix_rate / 10;
	}
	int n = (int)(want - frames_out);
	if (n <= 0) {
		return;
	}
	mix.resize(n * 2);
	out.resize(n * 2);
	audio_server_process(n, mix.ptrw());
	const int32_t *s = mix.ptr();
	float *d = out.ptrw();
	for (int i = 0; i < n * 2; i++) {
		d[i] = float(s[i] >> 16) / 32768.0f;
	}
	gasm_audio_push(out.ptr(), (uint32_t)n);
	frames_out = want;
}
