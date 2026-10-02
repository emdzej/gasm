# ScummVM on gasm: backend objects (copied to backends/platform/gasm/ by scripts/fetch-scummvm.sh).
# SPDX-License-Identifier: MIT
MODULE := backends/platform/gasm

MODULE_OBJS := \
	gasm-system.o \
	gasm-graphics.o \
	gasm-fs.o \
	gasm-saves.o \
	gasm-data-archive.o \
	gasm-data.o \
	gasm-loop.o \
	gasm-main.o

# backends are listed in OBJS directly rather than through rules.mk
MODULE_OBJS := $(addprefix $(MODULE)/, $(MODULE_OBJS))
OBJS := $(MODULE_OBJS) $(OBJS)
MODULE_DIRS += $(sort $(dir $(MODULE_OBJS)))
