/**************************************************************************/
/*  display_server_gasm.h                                                 */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
/* One window, the gasm drawable: GL through gasm:gl, input from gasm's raw
 * keyboard, text, pointer and gamepads. Everything a desktop has beyond that
 * (several windows, screens, IME, ...) is the headless server's no-op. */
#pragma once

#include "core/input/input_enums.h"
#include "servers/display/display_server_headless.h"

class DisplayServerGasm : public DisplayServerHeadless {
	GDSOFTCLASS(DisplayServerGasm, DisplayServerHeadless);

	bool gl = false;
	Callable window_event_callback;
	Callable input_text_callback;
	Point2i mouse_pos;
	BitField<MouseButtonMask> buttons;
	DisplayServerEnums::MouseMode mouse_mode = DisplayServerEnums::MOUSE_MODE_VISIBLE;
	bool joy_connected[4] = {};
	float joy_buttons[4][32] = {};
	float joy_axes[4][16] = {};

	static DisplayServer *create_func(const String &p_rendering_driver, DisplayServerEnums::WindowMode p_mode, DisplayServerEnums::VSyncMode p_vsync_mode, uint32_t p_flags, const Vector2i *p_position, const Vector2i &p_resolution, int p_screen, DisplayServerEnums::Context p_context, int64_t p_parent_window, Error &r_error);
	static Vector<String> get_rendering_drivers_func();

	void process_keys();
	void process_pointer();
	void process_joypads();
	void apply_mouse_mode();

public:
	static void register_gasm_driver();

	String get_name() const override { return "gasm"; }
	bool has_feature(DisplayServerEnums::Feature p_feature) const override;

	int get_screen_count() const override { return 1; }
	Size2i screen_get_size(int p_screen = DisplayServerEnums::SCREEN_OF_MAIN_WINDOW) const override { return window_get_size(); }
	Rect2i screen_get_usable_rect(int p_screen = DisplayServerEnums::SCREEN_OF_MAIN_WINDOW) const override { return Rect2i(Point2i(), window_get_size()); }
	float screen_get_refresh_rate(int p_screen = DisplayServerEnums::SCREEN_OF_MAIN_WINDOW) const override { return 60.0; }
	Vector<DisplayServerEnums::WindowID> get_window_list() const override;
	Size2i window_get_size(DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) const override;
	Size2i window_get_size_with_decorations(DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) const override { return window_get_size(); }
	int window_get_current_screen(DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) const override { return 0; }
	void window_set_title(const String &p_title, DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) override;
	void window_set_window_event_callback(const Callable &p_callable, DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) override { window_event_callback = p_callable; }
	void window_set_input_text_callback(const Callable &p_callable, DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) override { input_text_callback = p_callable; }
	bool window_is_focused(DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) const override { return true; }
	bool window_can_draw(DisplayServerEnums::WindowID p_window = DisplayServerEnums::MAIN_WINDOW_ID) const override { return true; }
	bool can_any_window_draw() const override { return true; }

	void mouse_set_mode(DisplayServerEnums::MouseMode p_mode) override;
	DisplayServerEnums::MouseMode mouse_get_mode() const override { return mouse_mode; }
	Point2i mouse_get_position() const override { return mouse_pos; }
	BitField<MouseButtonMask> mouse_get_button_state() const override { return buttons; }

	static DisplayServerGasm *get_singleton_gasm() { return static_cast<DisplayServerGasm *>(get_singleton()); }
	void send_close_request();

	void process_events() override;
	void swap_buffers() override;

	DisplayServerGasm(bool p_gl);
	~DisplayServerGasm() override;
};
