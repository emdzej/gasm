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
	// exactly rate / frame_rate frames on average: whole frames, no drift
	ticks++;
	uint64_t want = (uint64_t)((double)ticks * (double)mix_rate / p_frame_rate);
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
