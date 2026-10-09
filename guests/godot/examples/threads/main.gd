extends Node2D
# WorkerThreadPool on gasm, the loading case of design/threads.md: eight compressed
# blocks unpacked one after another on the main thread, then all at once as a group task.
# With the threaded engine natively (gasm-run godot-mt.wasm, --threads N) the tasks run
# on OS threads; with none allowed (headless runs, browsers, godot.wasm) the pool runs
# them on the main thread, so the results (and hashes) are the same everywhere.
#
# The data is text-like (16 symbols from a fixed generator, ~4 bits per byte), repeating
# only every 256 KiB, beyond deflate's 32 KiB window: unpacking it is real work.

const BLOCKS := 8
const SIZE := 8 * 1024 * 1024
const BASE := 256 * 1024
const MODE := FileAccess.COMPRESSION_DEFLATE
var packed := []
var out := []
var lock := Mutex.new()

func _ready() -> void:
	var base := PackedByteArray()
	base.resize(BASE)
	var x := 12345
	for k in BASE:
		x = (x * 1103515245 + 12345) % 2147483648
		base[k] = 97 + ((x >> 16) & 15)
	for i in BLOCKS:
		var off := i * 9973
		var raw := base.slice(off) + base.slice(0, off)
		while raw.size() < SIZE:
			raw.append_array(raw)
		raw.resize(SIZE)
		packed.append(raw.compress(MODE))
	out.resize(BLOCKS)
	var t0 := Time.get_ticks_usec()
	var serial := []
	for b in packed:
		serial.append(b.decompress(SIZE, MODE))
	var t1 := Time.get_ticks_usec()
	var group := WorkerThreadPool.add_group_task(_unpack, BLOCKS, -1, true)
	WorkerThreadPool.wait_for_group_task_completion(group)
	var t2 := Time.get_ticks_usec()
	var same := true
	for i in BLOCKS:
		same = same and out[i] == serial[i]
	var workers := OS.get_processor_count()
	print("threads: %d processors; results %s; main thread %d ms, pool %d ms" % [workers, "agree" if same else "DIFFER", (t1 - t0) / 1000, (t2 - t1) / 1000])
	$Status.text = "%d blocks of %d MiB: results %s" % [BLOCKS, SIZE / 1048576, "agree" if same else "DIFFER"]

func _unpack(i: int) -> void:
	var r: PackedByteArray = packed[i].decompress(SIZE, MODE)
	lock.lock()
	out[i] = r
	lock.unlock()
