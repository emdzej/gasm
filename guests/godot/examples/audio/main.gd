extends Node2D
# Audio on gasm: Godot mixes, the runner plays (gasm.audio_push), headless runs
# hash the samples.

const RATE := 44100.0
const MELODY := [0, 4, 7, 12, 7, 4, 0, -5, 0, 4, 7, 11, 14, 11, 7, 4]
var playback: AudioStreamGeneratorPlayback
var phase := 0.0
var note_index := 0
var note_left := 0.0
var spectrum: AudioEffectSpectrumAnalyzerInstance
var blip: AudioStreamWAV
var players: Array[AudioStreamPlayer] = []

func _ready() -> void:
	# a bus with reverb and a spectrum analyzer
	AudioServer.add_bus(1)
	AudioServer.set_bus_name(1, "FX")
	var reverb := AudioEffectReverb.new()
	reverb.room_size = 0.6
	reverb.wet = 0.25
	AudioServer.add_bus_effect(1, reverb)
	AudioServer.add_bus_effect(0, AudioEffectSpectrumAnalyzer.new())
	spectrum = AudioServer.get_bus_effect_instance(0, 0)

	var gen := AudioStreamGenerator.new()
	gen.mix_rate = RATE
	gen.buffer_length = 0.2
	$Melody.stream = gen
	$Melody.bus = "FX"
	$Melody.play()
	playback = $Melody.get_stream_playback()

	blip = make_blip()
	for i in 4:
		var p := AudioStreamPlayer.new()
		p.stream = blip
		p.bus = "FX"
		add_child(p)
		players.append(p)
	print("audio: ready, mix rate %d" % AudioServer.get_mix_rate())

# a short decaying square wave, 16-bit mono
func make_blip() -> AudioStreamWAV:
	var n := int(RATE * 0.25)
	var data := PackedByteArray()
	data.resize(n * 2)
	for i in n:
		var t := i / RATE
		var v := (1.0 if fmod(t * 660.0, 1.0) < 0.5 else -1.0) * exp(-t * 14.0) * 0.4
		data.encode_s16(i * 2, int(v * 32767.0))
	var w := AudioStreamWAV.new()
	w.format = AudioStreamWAV.FORMAT_16_BITS
	w.mix_rate = int(RATE)
	w.data = data
	return w

func _process(delta: float) -> void:
	# keep the generator's buffer full: a soft triangle melody, 0.2 s per note
	var frames := playback.get_frames_available()
	for i in frames:
		if note_left <= 0.0:
			note_index = (note_index + 1) % MELODY.size()
			note_left = 0.2
		note_left -= 1.0 / RATE
		var f := 220.0 * pow(2.0, MELODY[note_index] / 12.0)
		phase = fmod(phase + f / RATE, 1.0)
		var tri: float = 4.0 * absf(phase - 0.5) - 1.0
		var env: float = minf(1.0, note_left * 40.0)
		playback.push_frame(Vector2.ONE * tri * 0.18 * env)
	for k in 8:
		if Input.is_physical_key_pressed(KEY_1 + k) and not players[k % 4].playing:
			players[k % 4].pitch_scale = pow(2.0, [0, 2, 4, 5, 7, 9, 11, 12][k] / 12.0)
			players[k % 4].play()
			print("audio: note %d at frame %d" % [k + 1, Engine.get_process_frames()])
	queue_redraw()

func _draw() -> void:
	draw_rect(Rect2(0, 0, 640, 360), Color(0.08, 0.09, 0.14))
	var bars := 32
	for i in bars:
		var lo := 40.0 * pow(400.0, float(i) / bars)
		var hi := 40.0 * pow(400.0, float(i + 1) / bars)
		var m := spectrum.get_magnitude_for_frequency_range(lo, hi).length()
		var h: float = clamp(m * 900.0, 2.0, 280.0)
		draw_rect(Rect2(20 + i * 19, 320 - h, 15, h), Color.from_hsv(float(i) / bars, 0.6, 0.95))
	draw_string(ThemeDB.fallback_font, Vector2(20, 30), "Keys 1-8 play notes; the melody is generated sample by sample", HORIZONTAL_ALIGNMENT_LEFT, -1, 14)
