/**************************************************************************/
/*  os_gasm.h                                                             */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#pragma once

#include "audio_driver_gasm.h"

#include "core/os/os.h"

class OS_Gasm : public OS {
	MainLoop *main_loop = nullptr;
	AudioDriverGasm audio_driver;

protected:
	void initialize() override;
	void initialize_joypads() override {}
	void set_main_loop(MainLoop *p_main_loop) override;
	void delete_main_loop() override;
	void finalize() override;
	void finalize_core() override {}
	bool _check_internal_feature_support(const String &p_feature) override;

public:
	static OS_Gasm *get_singleton() { return static_cast<OS_Gasm *>(OS::get_singleton()); }

	bool main_loop_iterate();
	MainLoop *get_main_loop() const override { return main_loop; }

	Vector<String> get_video_adapter_driver_info() const override { return Vector<String>(); }
	String get_stdin_string(int64_t p_buffer_size = 1024) override { return String(); }
	PackedByteArray get_stdin_buffer(int64_t p_buffer_size = 1024) override { return PackedByteArray(); }
	Error get_entropy(uint8_t *r_buffer, int p_bytes) override;

	Error execute(const String &p_path, const List<String> &p_arguments, String *r_pipe = nullptr, int *r_exitcode = nullptr, bool read_stderr = false, Mutex *p_pipe_mutex = nullptr, bool p_open_console = false) override { return ERR_UNAVAILABLE; }
	Error create_process(const String &p_path, const List<String> &p_arguments, ProcessID *r_child_id = nullptr, bool p_open_console = false) override { return ERR_UNAVAILABLE; }
	Error kill(const ProcessID &p_pid) override { return ERR_UNAVAILABLE; }
	bool is_process_running(const ProcessID &p_pid) const override { return false; }
	int get_process_exit_code(const ProcessID &p_pid) const override { return -1; }
	int get_process_id() const override { return 1; }

	bool has_environment(const String &p_var) const override { return false; }
	String get_environment(const String &p_var) const override { return String(); }
	void set_environment(const String &p_var, const String &p_value) const override {}
	void unset_environment(const String &p_var) const override {}

	String get_name() const override { return "gasm"; }
	String get_distribution_name() const override { return "gasm"; }
	String get_version() const override { return String(); }

	DateTime get_datetime(bool p_utc = false) const override;
	TimeZoneInfo get_time_zone_info() const override;
	double get_unix_time() const override;
	void delay_usec(uint32_t p_usec) const override {}
	uint64_t get_ticks_usec() const override;
	void add_frame_delay(bool p_can_draw, bool p_wake_for_events) override {} // the runner paces frames

	String get_user_data_dir(const String &p_user_dir) const override { return "/user"; }
	String get_data_path() const override { return "/user"; }
	String get_config_path() const override { return "/user"; }
	String get_cache_path() const override { return "/user/cache"; }
	String get_executable_path() const override { return "/godot.wasm"; }
	bool is_userfs_persistent() const override { return true; }
	// a threaded build (wasi-threads) starts as many workers as the runner allows
	// (gasm.max_threads: 0 headless, so tasks run on the main thread and hashes compare)
	int get_processor_count() const override;
	int get_default_thread_pool_size() const override;

	void mix_audio(double p_frame_rate) { audio_driver.mix_frame(p_frame_rate); }

	OS_Gasm();
};
