/*
 * ScummVM on gasm: software graphics manager (see gasm-graphics.h).
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#define FORBIDDEN_SYMBOL_ALLOW_ALL
#include "common/scummsys.h"

#include "backends/platform/gasm/gasm-graphics.h"

#include "graphics/blit.h"
#include "graphics/pixelformat.h"

#include "gasm.h"

// bytes R, G, B, A in memory = what video_present takes
static const Graphics::PixelFormat kRGBA(4, 8, 8, 8, 8, 0, 8, 16, 24);
static const uint kOverlayW = 640, kOverlayH = 480;

/** 320x200 and 640x400 were shown on 4:3 monitors. */
static bool crtSize(uint w, uint h) {
	return (h == 200 && w == 320) || (h == 400 && w == 640);
}
/** Runners with gasm.video_set_aspect show such frames at 4:3 themselves. */
static bool runnerAspect() {
	static const bool supported = gasm_video_aspect(0, 0);
	return supported;
}
/** The height presented: on older runners rows are stretched by 6/5 here. */
static uint aspectHeight(uint w, uint h) {
	return !runnerAspect() && crtSize(w, h) ? h * 6 / 5 : h;
}

GasmGraphicsManager::GasmGraphicsManager()
	: _cursorPaletteSet(false), _cursorKey(0), _mouseVisible(false), _overlayVisible(false),
	  _shakeX(0), _shakeY(0), _screenChangeID(0) {
	memset(_palette, 0, sizeof _palette);
	memset(_cursorPalette, 0, sizeof _cursorPalette);
	_overlay.create(kOverlayW, kOverlayH, kRGBA);
	initSize(320, 200, nullptr);
}

GasmGraphicsManager::~GasmGraphicsManager() {
	_screen.free();
	_overlay.free();
	_cursor.free();
}

bool GasmGraphicsManager::hasFeature(OSystem::Feature f) const {
	return f == OSystem::kFeatureCursorPalette;
}

#ifdef USE_RGB_COLOR
Common::List<Graphics::PixelFormat> GasmGraphicsManager::getSupportedFormats() const {
	Common::List<Graphics::PixelFormat> list;
	list.push_back(kRGBA);
	list.push_back(Graphics::PixelFormat(2, 5, 6, 5, 0, 11, 5, 0, 0));   // RGB565
	list.push_back(Graphics::PixelFormat(2, 5, 5, 5, 0, 10, 5, 0, 0));   // RGB555
	list.push_back(Graphics::PixelFormat::createFormatCLUT8());
	return list;
}
#endif

void GasmGraphicsManager::initSize(uint width, uint height, const Graphics::PixelFormat *format) {
	Graphics::PixelFormat f = format ? *format : Graphics::PixelFormat::createFormatCLUT8();
	if (_screen.getPixels() && _screen.w == (int)width && _screen.h == (int)height && _screen.format == f)
		return;
	_screen.free();
	_screen.create(width, height, f);
	_screenChangeID++;
}

void GasmGraphicsManager::setPalette(const byte *colors, uint start, uint num) {
	memcpy(_palette + start * 3, colors, num * 3);
}

void GasmGraphicsManager::grabPalette(byte *colors, uint start, uint num) const {
	memcpy(colors, _palette + start * 3, num * 3);
}

void GasmGraphicsManager::setCursorPalette(const byte *colors, uint start, uint num) {
	memcpy(_cursorPalette + start * 3, colors, num * 3);
	_cursorPaletteSet = true;
}

void GasmGraphicsManager::copyRectToScreen(const void *buf, int pitch, int x, int y, int w, int h) {
	_screen.copyRectToSurface(buf, pitch, x, y, w, h);
}

void GasmGraphicsManager::fillScreen(uint32 col) {
	_screen.fillRect(Common::Rect(_screen.w, _screen.h), col);
}

void GasmGraphicsManager::fillScreen(const Common::Rect &r, uint32 col) {
	_screen.fillRect(r, col);
}

/** The GUI draws on top of the game: start the overlay as the game screen, scaled up. */
void GasmGraphicsManager::clearOverlay() {
	uint w = _screen.w, outH = aspectHeight(w, _screen.h);
	Common::Array<uint32> game(w * outH);
	blitScreen(game.data(), outH);
	for (int y = 0; y < _overlay.h; y++) {
		uint32 *dst = (uint32 *)_overlay.getBasePtr(0, y);
		const uint32 *src = &game[(y * outH / _overlay.h) * w];
		for (int x = 0; x < _overlay.w; x++)
			dst[x] = src[x * w / _overlay.w];
	}
}

void GasmGraphicsManager::grabOverlay(Graphics::Surface &surface) const {
	surface.copyFrom(_overlay);
}

void GasmGraphicsManager::copyRectToOverlay(const void *buf, int pitch, int x, int y, int w, int h) {
	_overlay.copyRectToSurface(buf, pitch, x, y, w, h);
}

bool GasmGraphicsManager::showMouse(bool visible) {
	bool was = _mouseVisible;
	_mouseVisible = visible;
	return was;
}

void GasmGraphicsManager::setMouseCursor(const void *buf, uint w, uint h, int hotspotX, int hotspotY, uint32 keycolor,
                                         bool, const Graphics::PixelFormat *format, const byte *) {
	Graphics::PixelFormat f = format ? *format : Graphics::PixelFormat::createFormatCLUT8();
	_cursor.free();
	if (!w || !h)
		return;
	_cursor.create(w, h, f);
	_cursor.copyRectToSurface(buf, w * f.bytesPerPixel, 0, 0, w, h);
	_cursorHotspot = Common::Point(hotspotX, hotspotY);
	_cursorKey = keycolor;
}

Common::Point GasmGraphicsManager::frameToScreen(float fx, float fy) const {
	int w = _overlayVisible ? _overlay.w : _screen.w;
	int h = _overlayVisible ? _overlay.h : _screen.h;
	uint outH = _overlayVisible ? h : aspectHeight(w, h);
	int x = (int)fx, y = (int)(fy * h / (float)outH);
	return Common::Point(CLIP(x, 0, w - 1), CLIP(y, 0, h - 1));
}

/** The game screen into `out` (screen width x outH), converted to RGBA, shaken. */
void GasmGraphicsManager::blitScreen(uint32 *out, uint outH) {
	uint w = _screen.w, h = _screen.h;
	static Common::Array<uint32> row;
	row.resize(w);
	// the palette as RGBA, once per frame (not per pixel lookup of 3 bytes)
	uint32 pal[256];
	if (_screen.format.bytesPerPixel == 1)
		for (uint i = 0; i < 256; i++)
			pal[i] = rgba(_palette[i * 3], _palette[i * 3 + 1], _palette[i * 3 + 2]);
	int lastSy = -1;
	for (uint oy = 0; oy < outH; oy++) {
		int sy = (int)(oy * h / outH) - _shakeY;
		uint32 *dst = out + oy * w;
		if (sy < 0 || sy >= (int)h) {
			memset(dst, 0, w * 4);
			lastSy = -1;
			continue;
		}
		if (sy == lastSy) {   // aspect correction repeats source rows: copy the row just made
			memcpy(dst, dst - w, w * 4);
			continue;
		}
		lastSy = sy;
		const byte *src = (const byte *)_screen.getBasePtr(0, sy);
		// without shake, convert straight into the output row
		uint32 *conv = _shakeX == 0 ? dst : row.data();
		if (_screen.format.bytesPerPixel == 1) {
			for (uint x = 0; x < w; x++)
				conv[x] = pal[src[x]];
		} else {
			Graphics::crossBlit((byte *)conv, src, w * 4, _screen.pitch, w, 1, kRGBA, _screen.format);
			for (uint x = 0; x < w; x++)
				conv[x] |= 0xffu << 24;
		}
		if (_shakeX != 0) {
			for (uint x = 0; x < w; x++) {
				int sx = (int)x - _shakeX;
				dst[x] = sx >= 0 && sx < (int)w ? row[sx] : rgba(0, 0, 0);
			}
		}
	}
}

void GasmGraphicsManager::drawCursor(uint32 *out, uint w, uint h, uint outH, uint srcH) {
	if (!_mouseVisible || !_cursor.getPixels())
		return;
	const byte *pal = _cursorPaletteSet ? _cursorPalette : _palette;
	int x0 = _mouse.x - _cursorHotspot.x, y0 = _mouse.y - _cursorHotspot.y;
	for (int cy = 0; cy < _cursor.h; cy++) {
		// the cursor stretches with the screen it's drawn on
		int sy = y0 + cy;
		uint oy0 = (uint)((int64)sy * outH / srcH), oy1 = (uint)((int64)(sy + 1) * outH / srcH);
		if (sy < 0 || oy0 >= outH)
			continue;
		for (int cx = 0; cx < _cursor.w; cx++) {
			int ox = x0 + cx;
			if (ox < 0 || ox >= (int)w)
				continue;
			uint32 c;
			if (_cursor.format.bytesPerPixel == 1) {
				byte idx = *(const byte *)_cursor.getBasePtr(cx, cy);
				if (idx == _cursorKey)
					continue;
				c = rgba(pal[idx * 3], pal[idx * 3 + 1], pal[idx * 3 + 2]);
			} else {
				uint32 v = _cursor.getPixel(cx, cy);
				if (v == _cursorKey)
					continue;
				byte a, r, g, b;
				_cursor.format.colorToARGB(v, a, r, g, b);
				if (_cursor.format.aBits() && a < 128)
					continue;
				c = rgba(r, g, b);
			}
			for (uint oy = oy0; oy < oy1 && oy < outH; oy++)
				out[oy * w + ox] = c;
		}
	}
	(void)h;
}

void GasmGraphicsManager::updateScreen() {
	uint w, srcH, outH;
	if (_overlayVisible) {
		w = _overlay.w;
		srcH = outH = _overlay.h;
		_frame.resize(w * outH);
		for (uint y = 0; y < outH; y++)
			memcpy(&_frame[y * w], _overlay.getBasePtr(0, y), w * 4);
	} else {
		w = _screen.w;
		srcH = _screen.h;
		outH = aspectHeight(w, srcH);
		_frame.resize(w * outH);
		blitScreen(_frame.data(), outH);
	}
	drawCursor(_frame.data(), w, srcH, outH, srcH);
	if (runnerAspect()) {
		bool crt = !_overlayVisible && crtSize(w, srcH);
		gasm_video_set_aspect(crt ? 4 : 0, crt ? 3 : 0);
	}
	gasm_video_present(_frame.data(), w, outH, w * 4);
}
