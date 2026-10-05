/**************************************************************************/
/*  display_server_gasm.cpp                                               */
/**************************************************************************/
/* Godot on gasm (https://gasm.emdzej.pl): MIT, like Godot.                */
#include "display_server_gasm.h"

#include "core/input/input.h"
#include "core/input/input_event.h"
#include "core/os/keyboard.h"
#include "servers/rendering/dummy/rasterizer_dummy.h"

#ifdef GLES3_ENABLED
#include "drivers/gles3/rasterizer_gles3.h"
#endif

#include "gasm.h"

#include <string.h>

// Godot's web platform maps W3C KeyboardEvent.code names to keys; gasm's keys are those names.
typedef char EM_UTF8;
#include "platform/web/dom_keys.inc"

static Key key_from_code(uint32_t p_code, KeyLocation *r_location) {
	char name[32] = {};
	strncpy(name, gasm_key_name(p_code), sizeof name - 1);
	*r_location = dom_code2godot_key_location(name);
	return dom_code2godot_scancode(name, name, true);
}

Vector<String> DisplayServerGasm::get_rendering_drivers_func() {
	Vector<String> drivers;
#ifdef GLES3_ENABLED
	drivers.push_back("opengl3");
#endif
	drivers.push_back("dummy");
	return drivers;
}

DisplayServer *DisplayServerGasm::create_func(const String &p_rendering_driver, DisplayServerEnums::WindowMode p_mode, DisplayServerEnums::VSyncMode p_vsync_mode, uint32_t p_flags, const Vector2i *p_position, const Vector2i &p_resolution, int p_screen, DisplayServerEnums::Context p_context, int64_t p_parent_window, Error &r_error) {
	r_error = OK;
	bool gl = false;
#ifdef GLES3_ENABLED
	// gasm:gl is there if the runner gives it (every runner does; the null GL headless)
	gl = p_rendering_driver != "dummy" && gasm_has_str("gasm:gl");
	if (gl) {
		// as Emscripten does: every WebGL extension on (Godot checks GL_EXTENSIONS)
		int32_t n = gasm_gl_get_string(0x1F03, nullptr, 0);
		if (n > 0) {
			CharString exts;
			exts.resize_uninitialized(n + 1);
			gasm_gl_get_string(0x1F03, exts.ptrw(), n);
			exts.ptrw()[n] = 0;
			for (const String &e : String(exts.ptr()).split(" ", false)) {
				CharString c = e.utf8();
				gasm_gl_enable_extension(c.ptr(), c.length());
			}
		}
		RasterizerGLES3::make_current(false);
	} else
#endif
	{
		RasterizerDummy::make_current();
	}
	return memnew(DisplayServerGasm(gl));
}

void DisplayServerGasm::register_gasm_driver() {
	register_create_function("gasm", create_func, get_rendering_drivers_func);
}

bool DisplayServerGasm::has_feature(DisplayServerEnums::Feature p_feature) const {
	switch (p_feature) {
		case DisplayServerEnums::FEATURE_MOUSE:
		case DisplayServerEnums::FEATURE_CURSOR_SHAPE:
			return true;
		default:
			return false;
	}
}

Vector<DisplayServerEnums::WindowID> DisplayServerGasm::get_window_list() const {
	Vector<DisplayServerEnums::WindowID> list;
	list.push_back(DisplayServerEnums::MAIN_WINDOW_ID);
	return list;
}

Size2i DisplayServerGasm::window_get_size(DisplayServerEnums::WindowID p_window) const {
#ifdef GLES3_ENABLED
	if (gl) {
		return Size2i(gasm_gl_width(), gasm_gl_height());
	}
#endif
	return Size2i(1280, 720);
}

void DisplayServerGasm::window_set_title(const String &p_title, DisplayServerEnums::WindowID p_window) {
	gasm_set_title_str(p_title.utf8().ptr());
}

void DisplayServerGasm::apply_mouse_mode() {
	uint32_t flags = GASM_INPUT_KEYS_RAW; // Godot reads the keyboard itself
	if (mouse_mode == DisplayServerEnums::MOUSE_MODE_HIDDEN || mouse_mode == DisplayServerEnums::MOUSE_MODE_CONFINED_HIDDEN) {
		flags |= GASM_INPUT_POINTER_HIDDEN;
	}
	if (mouse_mode == DisplayServerEnums::MOUSE_MODE_CAPTURED) {
		flags |= GASM_INPUT_POINTER_HIDDEN | GASM_INPUT_POINTER_LOCKED;
	}
	gasm_input_mode(flags);
}

void DisplayServerGasm::mouse_set_mode(DisplayServerEnums::MouseMode p_mode) {
	mouse_mode = p_mode;
	apply_mouse_mode();
}

void DisplayServerGasm::process_keys() {
	Input *input = Input::get_singleton();
	uint8_t keys[GASM_KEY_STATE_BYTES] = {};
	bool have_state = gasm_key_state(keys, sizeof keys) >= 0;
	auto held = [&](uint32_t k) { return have_state && (keys[k / 8] >> (k % 8)) & 1; };
	int32_t len = gasm_key_events(nullptr, 0);
	if (len > 0) {
		Vector<uint8_t> ev;
		ev.resize(len);
		gasm_key_events(ev.ptrw(), len);
		for (int i = 0; i + GASM_KEY_EVENT_BYTES <= len; i += GASM_KEY_EVENT_BYTES) {
			uint32_t code = ev[i] | (ev[i + 1] << 8);
			bool down = ev[i + 2] != 0;
			KeyLocation location = KeyLocation::UNSPECIFIED;
			Key key = key_from_code(code, &location);
			if (key == Key::NONE) {
				continue;
			}
			Ref<InputEventKey> k;
			k.instantiate();
			k->set_pressed(down);
			k->set_keycode(key);
			k->set_physical_keycode(key);
			k->set_key_label(key);
			k->set_location(location);
			k->set_shift_pressed(held(GASM_KEY_SHIFT_LEFT) || held(GASM_KEY_SHIFT_RIGHT));
			k->set_ctrl_pressed(held(GASM_KEY_CONTROL_LEFT) || held(GASM_KEY_CONTROL_RIGHT));
			k->set_alt_pressed(held(GASM_KEY_ALT_LEFT) || held(GASM_KEY_ALT_RIGHT));
			k->set_meta_pressed(held(GASM_KEY_META_LEFT) || held(GASM_KEY_META_RIGHT));
			input->parse_input_event(k);
		}
	}
	// typed text: key events with a character (what LineEdit and friends read)
	int32_t tlen = gasm_text_input(nullptr, 0);
	if (tlen > 0) {
		CharString t;
		t.resize_uninitialized(tlen + 1);
		gasm_text_input(t.ptrw(), tlen);
		t.ptrw()[tlen] = 0;
		String s = String::utf8(t.ptr(), tlen);
		for (int i = 0; i < s.length(); i++) {
			char32_t c = s[i];
			if (c < 32 || c == 127) {
				continue; // Enter, Backspace, ...: key events above
			}
			Ref<InputEventKey> k;
			k.instantiate();
			k->set_pressed(true);
			k->set_unicode(c);
			input->parse_input_event(k);
		}
	}
}

void DisplayServerGasm::process_pointer() {
	uint8_t p[GASM_POINTER_BYTES];
	if (gasm_pointer(p, sizeof p) < 0) {
		return;
	}
	auto f32 = [&](int off) { float v; memcpy(&v, p + off, 4); return v; };
	auto u32 = [&](int off) { uint32_t v; memcpy(&v, p + off, 4); return v; };
	Input *input = Input::get_singleton();
	Point2i pos(f32(GASM_POINTER_OFF_X), f32(GASM_POINTER_OFF_Y));
	Vector2 rel(f32(GASM_POINTER_OFF_DX), f32(GASM_POINTER_OFF_DY));
	uint32_t held = u32(GASM_POINTER_OFF_BUTTONS), pressed = u32(GASM_POINTER_OFF_PRESSED), released = u32(GASM_POINTER_OFF_RELEASED);
	static const struct { uint32_t bit; MouseButton button; } MAP[] = {
		{ GASM_MOUSE_LEFT, MouseButton::LEFT }, { GASM_MOUSE_RIGHT, MouseButton::RIGHT }, { GASM_MOUSE_MIDDLE, MouseButton::MIDDLE },
		{ GASM_MOUSE_BACK, MouseButton::MB_XBUTTON1 }, { GASM_MOUSE_FORWARD, MouseButton::MB_XBUTTON2 },
	};
	BitField<MouseButtonMask> mask;
	for (const auto &m : MAP) {
		if (held & m.bit) {
			mask.set_flag(mouse_button_to_mask(m.button));
		}
	}
	if (pos != mouse_pos || rel != Vector2()) {
		Ref<InputEventMouseMotion> mm;
		mm.instantiate();
		mm->set_position(pos);
		mm->set_global_position(pos);
		mm->set_relative(rel);
		mm->set_relative_screen_position(rel);
		mm->set_button_mask(mask);
		input->set_mouse_position(pos);
		input->parse_input_event(mm);
	}
	mouse_pos = pos;
	for (const auto &m : MAP) {
		for (int pass = 0; pass < 2; pass++) { // a press and release in one frame: both, in order
			bool down = pass == 0;
			if (!((down ? pressed : released) & m.bit)) {
				continue;
			}
			Ref<InputEventMouseButton> mb;
			mb.instantiate();
			mb->set_position(pos);
			mb->set_global_position(pos);
			mb->set_button_index(m.button);
			mb->set_pressed(down);
			if (down) {
				buttons.set_flag(mouse_button_to_mask(m.button));
			} else {
				buttons.clear_flag(mouse_button_to_mask(m.button));
			}
			mb->set_button_mask(buttons);
			input->parse_input_event(mb);
		}
	}
	buttons = mask;
	float wx = f32(GASM_POINTER_OFF_WHEEL_X), wy = f32(GASM_POINTER_OFF_WHEEL_Y);
	auto wheel = [&](float amount, MouseButton neg, MouseButton posb) {
		if (amount == 0) {
			return;
		}
		for (int pass = 0; pass < 2; pass++) {
			Ref<InputEventMouseButton> mb;
			mb.instantiate();
			mb->set_position(pos);
			mb->set_global_position(pos);
			mb->set_button_index(amount < 0 ? neg : posb);
			mb->set_factor(Math::abs(amount));
			mb->set_pressed(pass == 0);
			mb->set_button_mask(buttons);
			input->parse_input_event(mb);
		}
	};
	wheel(wy, MouseButton::WHEEL_UP, MouseButton::WHEEL_DOWN); // gasm: positive y scrolls down
	wheel(wx, MouseButton::WHEEL_LEFT, MouseButton::WHEEL_RIGHT);
}

void DisplayServerGasm::process_joypads() {
	Input *input = Input::get_singleton();
	for (int slot = 0; slot < 4; slot++) {
		uint8_t g[GASM_GAMEPAD_BYTES];
		bool connected = gasm_gamepad(slot, g, sizeof g) >= 0 && (g[GASM_GAMEPAD_OFF_FLAGS] & 1);
		if (connected != joy_connected[slot]) {
			char name[64] = "gamepad";
			int32_t n = gasm_gamepad_name(slot, name, sizeof name - 1);
			if (n > 0 && n < (int32_t)sizeof name) {
				name[n] = 0;
			}
			input->joy_connection_changed(slot, connected, connected ? String::utf8(name) : String());
			joy_connected[slot] = connected;
		}
		if (!connected) {
			continue;
		}
		uint32_t nb, na;
		memcpy(&nb, g + GASM_GAMEPAD_OFF_BUTTON_COUNT, 4);
		memcpy(&na, g + GASM_GAMEPAD_OFF_AXIS_COUNT, 4);
		// W3C "standard" gamepad layout to Godot's (the same as Godot's web platform)
		static const JoyButton BUTTONS[17] = { JoyButton::A, JoyButton::B, JoyButton::X, JoyButton::Y, JoyButton::LEFT_SHOULDER,
			JoyButton::RIGHT_SHOULDER, JoyButton::INVALID, JoyButton::INVALID, JoyButton::BACK, JoyButton::START,
			JoyButton::LEFT_STICK, JoyButton::RIGHT_STICK, JoyButton::DPAD_UP, JoyButton::DPAD_DOWN, JoyButton::DPAD_LEFT,
			JoyButton::DPAD_RIGHT, JoyButton::GUIDE };
		for (uint32_t b = 0; b < MIN(nb, 32u); b++) {
			float v;
			memcpy(&v, g + GASM_GAMEPAD_OFF_BUTTONS + b * 4, 4);
			if (v == joy_buttons[slot][b]) {
				continue;
			}
			joy_buttons[slot][b] = v;
			if (b == 6 || b == 7) { // triggers are axes in Godot
				input->joy_axis(slot, b == 6 ? JoyAxis::TRIGGER_LEFT : JoyAxis::TRIGGER_RIGHT, v);
			} else if (b < 17 && BUTTONS[b] != JoyButton::INVALID) {
				input->joy_button(slot, BUTTONS[b], v > 0.5f);
			}
		}
		for (uint32_t a = 0; a < MIN(na, 4u); a++) {
			float v;
			memcpy(&v, g + GASM_GAMEPAD_OFF_AXES + a * 4, 4);
			if (v != joy_axes[slot][a]) {
				joy_axes[slot][a] = v;
				input->joy_axis(slot, (JoyAxis)a, v); // LEFT_X, LEFT_Y, RIGHT_X, RIGHT_Y
			}
		}
	}
}

void DisplayServerGasm::process_events() {
	Size2i size = window_get_size();
	if (size != last_size && size.x > 0 && size.y > 0) {
		last_size = size;
		if (rect_changed_callback.is_valid()) {
			rect_changed_callback.call(Rect2i(Point2i(), size));
		}
	}
	process_keys();
	process_pointer();
	process_joypads();
	Input::get_singleton()->flush_buffered_events();
}

void DisplayServerGasm::send_close_request() {
	if (window_event_callback.is_valid()) {
		window_event_callback.call(DisplayServerEnums::WINDOW_EVENT_CLOSE_REQUEST);
	}
}

void DisplayServerGasm::swap_buffers() {
#ifdef GLES3_ENABLED
	if (gl) {
		gasm_gl_present();
	}
#endif
}

DisplayServerGasm::DisplayServerGasm(bool p_gl) :
		gl(p_gl) {
	last_size = window_get_size(); // Godot starts at this size: report changes only
	apply_mouse_mode();
}

DisplayServerGasm::~DisplayServerGasm() {}
