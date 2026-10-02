/*
 * ScummVM on gasm: a software graphics manager. The game screen (paletted,
 * 16 or 32 bit) or the GUI overlay plus the cursor are combined into one RGBA
 * frame for video_present. 320x200 and 640x400 screens are stretched to 4:3.
 *
 * SPDX-License-Identifier: MIT (this file; the ScummVM build as a whole is GPL-3.0)
 */
#ifndef BACKENDS_PLATFORM_GASM_GRAPHICS_H
#define BACKENDS_PLATFORM_GASM_GRAPHICS_H

#include "backends/graphics/graphics.h"
#include "common/array.h"
#include "common/rect.h"
#include "graphics/surface.h"

class GasmGraphicsManager : public GraphicsManager {
public:
	GasmGraphicsManager();
	~GasmGraphicsManager() override;

	bool hasFeature(OSystem::Feature f) const override;
	void setFeatureState(OSystem::Feature f, bool enable) override {}
	bool getFeatureState(OSystem::Feature f) const override { return false; }

#ifdef USE_RGB_COLOR
	Graphics::PixelFormat getScreenFormat() const override { return _screen.format; }
	Common::List<Graphics::PixelFormat> getSupportedFormats() const override;
#endif
	void initSize(uint width, uint height, const Graphics::PixelFormat *format = nullptr) override;
	int getScreenChangeID() const override { return _screenChangeID; }
	void beginGFXTransaction() override {}
	OSystem::TransactionError endGFXTransaction() override { return OSystem::kTransactionSuccess; }
	int16 getHeight() const override { return (int16)_screen.h; }
	int16 getWidth() const override { return (int16)_screen.w; }

	void setPalette(const byte *colors, uint start, uint num) override;
	void grabPalette(byte *colors, uint start, uint num) const override;

	void copyRectToScreen(const void *buf, int pitch, int x, int y, int w, int h) override;
	Graphics::Surface *lockScreen() override { return &_screen; }
	void unlockScreen() override {}
	void fillScreen(uint32 col) override;
	void fillScreen(const Common::Rect &r, uint32 col) override;
	void updateScreen() override;
	void setShakePos(int shakeXOffset, int shakeYOffset) override { _shakeX = shakeXOffset; _shakeY = shakeYOffset; }
	void setFocusRectangle(const Common::Rect &rect) override {}
	void clearFocusRectangle() override {}

	void showOverlay(bool inGUI) override { _overlayVisible = true; }
	void hideOverlay() override { _overlayVisible = false; }
	bool isOverlayVisible() const override { return _overlayVisible; }
	Graphics::PixelFormat getOverlayFormat() const override { return _overlay.format; }
	void clearOverlay() override;
	void grabOverlay(Graphics::Surface &surface) const override;
	void copyRectToOverlay(const void *buf, int pitch, int x, int y, int w, int h) override;
	int16 getOverlayHeight() const override { return (int16)_overlay.h; }
	int16 getOverlayWidth() const override { return (int16)_overlay.w; }

	bool showMouse(bool visible) override;
	void warpMouse(int x, int y) override { _mouse = Common::Point(x, y); }
	void setMouseCursor(const void *buf, uint w, uint h, int hotspotX, int hotspotY, uint32 keycolor,
	                    bool dontScale = false, const Graphics::PixelFormat *format = nullptr, const byte *mask = nullptr) override;
	void setCursorPalette(const byte *colors, uint start, uint num) override;

	/** Pointer position in presented-frame pixels -> screen (or overlay) coordinates. */
	Common::Point frameToScreen(float fx, float fy) const;
	Common::Point mousePosition() const { return _mouse; }
	void setMousePosition(const Common::Point &p) { _mouse = p; }

private:
	void blitScreen(uint32 *out, uint outH);
	void drawCursor(uint32 *out, uint w, uint h, uint outH, uint srcH);
	static uint32 rgba(byte r, byte g, byte b) { return r | g << 8 | b << 16 | 0xffu << 24; }

	Graphics::Surface _screen, _overlay, _cursor;
	byte _palette[256 * 3];
	byte _cursorPalette[256 * 3];
	bool _cursorPaletteSet;
	uint32 _cursorKey;
	Common::Point _cursorHotspot;
	Common::Point _mouse;
	bool _mouseVisible, _overlayVisible;
	int _shakeX, _shakeY;
	int _screenChangeID;
	Common::Array<uint32> _frame;
};

#endif
