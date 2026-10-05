/**************************************************************************/
/*  audio_driver_gasm.h                                                   */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#pragma once

#include "servers/audio/audio_server.h"

// Mixes the audio of each gasm frame (gasm_audio_push), from the frame loop: as much as
// time has passed (gasm_time_ms), so the sound keeps up when the game runs below its rate.
class AudioDriverGasm : public AudioDriver {
	int mix_rate = 44100;
	uint64_t frames_out = 0; // stereo frames pushed so far
	double start_ms = -1.0; // gasm_time_ms of the first frame
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

	// the samples for one frame at `p_frame_rate` frames per second: up to the frame's time
	void mix_frame(double p_frame_rate);
};
