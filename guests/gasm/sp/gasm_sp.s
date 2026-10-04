# The wasm stack pointer (__stack_pointer, a global the linker defines), for the
# cooperative thread scheduler (src/thread.rs): stable Rust can't read or set a
# wasm global. Assembled into libgasm_sp.a by scripts/build-rust-sp.sh; build.rs
# links it on wasm32.
	.globaltype __stack_pointer, i32

	.section .text.gasm__get_sp,"",@
	.hidden gasm__get_sp
	.globl gasm__get_sp
	.type gasm__get_sp,@function
gasm__get_sp:
	.functype gasm__get_sp () -> (i32)
	global.get __stack_pointer
	end_function

	.section .text.gasm__set_sp,"",@
	.hidden gasm__set_sp
	.globl gasm__set_sp
	.type gasm__set_sp,@function
gasm__set_sp:
	.functype gasm__set_sp (i32) -> ()
	local.get 0
	global.set __stack_pointer
	end_function
