/**************************************************************************/
/*  audio_driver_gasm.h                                                   */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#pragma once

#include "servers/audio/audio_server.h"

// Mixes one gasm frame of audio at a time (gasm_audio_push), from the frame loop.
class AudioDriverGasm : public AudioDriver {
	int mix_rate = 44100;
	uint64_t frames_out = 0; // stereo frames pushed so far
	uint64_t ticks = 0; // gasm frames so far
	Vector<int32_t> mix;
	Vector<float> out;
	bool active = false;

public:
	const char *get_name() const override { return "gasm"; }
	Error init() override;
	void start() override { active = true; }
	int get_mix_rate() const override { return mix_rate; }
	SpeakerMode get_speaker_mode() const override { return SPEAKER_MODE_STEREO; }
	float get_latency() override { return 1.0f / 60.0f; }
	void lock() override {}
	void unlock() override {}
	void finish() override { active = false; }

	// the samples for one frame at `p_frame_rate` frames per second
	void mix_frame(double p_frame_rate);
};
