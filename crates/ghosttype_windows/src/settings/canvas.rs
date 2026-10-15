//! Direct2D drawing with alpha into a window's device context, so content blends with Mica.
//!
//! GDI writes no alpha, so over the extended frame its pixels read as transparent. Direct2D draws
//! premultiplied pixels into a memory bitmap, and a copy to the window keeps their alpha, which
//! DWM then blends with the backdrop.

use windows::Win32::Foundation::RECT;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory,
    ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{BitBlt, HDC, SRCCOPY};
use windows::core::Result;
use windows_numerics::Vector2;

use crate::gdi::{MemoryContext, PixelSection};
use crate::pixels::Color;
use crate::settings::layout::Bounds;
use crate::settings::typography::{TextStyle, Typography, coordinate};

const PIXEL_DPI: f32 = 96.0;
const HALF: f32 = 0.5;

/// Creates the Direct2D targets that settings drawing goes through.
#[derive(Debug)]
pub(crate) struct Canvas {
    factory: ID2D1Factory,
}

impl Canvas {
    pub(crate) fn new() -> Result<Self> {
        // SAFETY: plain call.
        let factory = unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        Ok(Self { factory })
    }

    /// Fills `area` of `context` with `clear`, lets `draw` paint on top in coordinates relative
    /// to the area, and copies the result with its alpha.
    pub(crate) fn paint(
        &self,
        context: HDC,
        area: Bounds,
        clear: Color,
        typography: &Typography,
        draw: impl FnOnce(&Painter<'_>) -> Result<()>,
    ) -> Result<()> {
        if area.width <= 0 || area.height <= 0 {
            return Ok(());
        }
        let section = PixelSection::new(SIZE {
            cx: area.width,
            cy: area.height,
        })?;
        let memory = MemoryContext::new()?;
        let _bitmap = memory.select(section.object());
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: PIXEL_DPI,
            dpiY: PIXEL_DPI,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: `properties` is a local that outlives the call.
        let target = unsafe { self.factory.CreateDCRenderTarget(&raw const properties) }?;
        let bitmap_area = RECT {
            left: 0,
            top: 0,
            right: area.width,
            bottom: area.height,
        };
        // SAFETY: the memory context holds a 32-bit bitmap of this size for the whole drawing.
        unsafe { target.BindDC(memory.handle(), &raw const bitmap_area) }?;
        let clear = color_f(clear);
        // SAFETY: plain calls on the target just bound; drawing happens between BeginDraw and EndDraw.
        unsafe {
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            target.BeginDraw();
            target.Clear(Some(&raw const clear));
        }
        let drawn = draw(&Painter {
            target: &target,
            typography,
        });
        // SAFETY: ends the drawing that BeginDraw started.
        let ended = unsafe { target.EndDraw(None, None) };
        drawn?;
        ended?;
        // SAFETY: both contexts are alive and the bitmap covers the copied area.
        unsafe {
            BitBlt(
                context,
                area.x,
                area.y,
                area.width,
                area.height,
                Some(memory.handle()),
                0,
                0,
                SRCCOPY,
            )
        }
    }
}

/// Draws shapes and text in physical pixels while a [`Canvas::paint`] runs.
#[derive(Debug)]
pub(crate) struct Painter<'a> {
    target: &'a ID2D1DCRenderTarget,
    typography: &'a Typography,
}

impl Painter<'_> {
    /// Fills `bounds`.
    pub(crate) fn fill(&self, bounds: Bounds, color: Color) -> Result<()> {
        if color.alpha == 0 {
            return Ok(());
        }
        let brush = self.brush(color)?;
        let area = rect_f(bounds);
        // SAFETY: the rectangle and brush outlive the call.
        unsafe { self.target.FillRectangle(&raw const area, &brush) };
        Ok(())
    }

    /// Fills `bounds` with corners of `radius` pixels.
    pub(crate) fn fill_rounded(&self, bounds: Bounds, radius: f32, color: Color) -> Result<()> {
        if color.alpha == 0 {
            return Ok(());
        }
        let brush = self.brush(color)?;
        let shape = rounded(rect_f(bounds), radius);
        // SAFETY: the shape and brush outlive the call.
        unsafe { self.target.FillRoundedRectangle(&raw const shape, &brush) };
        Ok(())
    }

    /// Fills `bounds` as a pill: its corners are half its smaller side.
    pub(crate) fn fill_pill(&self, bounds: Bounds, color: Color) -> Result<()> {
        let radius = coordinate(bounds.width.min(bounds.height)) * HALF;
        self.fill_rounded(bounds, radius, color)
    }

    /// Draws a border of `width` pixels just inside `bounds`, with corners of `radius` pixels.
    pub(crate) fn stroke_rounded(&self, bounds: Bounds, radius: f32, width: i32, color: Color) -> Result<()> {
        if color.alpha == 0 || width <= 0 {
            return Ok(());
        }
        let brush = self.brush(color)?;
        let inset = coordinate(width) * HALF;
        let outer = rect_f(bounds);
        let shape = rounded(
            D2D_RECT_F {
                left: outer.left + inset,
                top: outer.top + inset,
                right: outer.right - inset,
                bottom: outer.bottom - inset,
            },
            (radius - inset).max(0.0),
        );
        // SAFETY: the shape and brush outlive the call.
        unsafe {
            self.target
                .DrawRoundedRectangle(&raw const shape, &brush, coordinate(width), None);
        }
        Ok(())
    }

    /// Fills the bottom `height` rows of the rounded shape of `bounds`, the way Fluent draws the
    /// stronger bottom edge of controls.
    pub(crate) fn bottom_edge(&self, bounds: Bounds, radius: f32, height: i32, color: Color) -> Result<()> {
        if color.alpha == 0 || height <= 0 {
            return Ok(());
        }
        let band = rect_f(Bounds {
            y: bounds.bottom() - height,
            height,
            ..bounds
        });
        // SAFETY: the clip rectangle outlives the call; it is popped right after the fill.
        unsafe {
            self.target
                .PushAxisAlignedClip(&raw const band, D2D1_ANTIALIAS_MODE_ALIASED);
        }
        let filled = self.fill_rounded(bounds, radius, color);
        // SAFETY: pops the clip pushed above.
        unsafe { self.target.PopAxisAlignedClip() };
        filled
    }

    /// Draws `text` in `style` inside `bounds`, cut at its edges.
    pub(crate) fn text(&self, text: &str, bounds: Bounds, style: TextStyle, color: Color) -> Result<()> {
        if text.is_empty() || color.alpha == 0 {
            return Ok(());
        }
        let layout = self.typography.layout(text, style, bounds)?;
        let brush = self.brush(color)?;
        let origin = Vector2 {
            X: coordinate(bounds.x),
            Y: coordinate(bounds.y),
        };
        // SAFETY: the layout and brush outlive the call.
        unsafe {
            self.target
                .DrawTextLayout(origin, &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
        Ok(())
    }

    fn brush(&self, color: Color) -> Result<ID2D1SolidColorBrush> {
        let color = color_f(color);
        // SAFETY: the color outlives the call.
        unsafe { self.target.CreateSolidColorBrush(&raw const color, None) }
    }
}

fn color_f(color: Color) -> D2D1_COLOR_F {
    let channel = |value: u8| f32::from(value) / f32::from(u8::MAX);
    D2D1_COLOR_F {
        r: channel(color.rgb.red),
        g: channel(color.rgb.green),
        b: channel(color.rgb.blue),
        a: channel(color.alpha),
    }
}

fn rect_f(bounds: Bounds) -> D2D_RECT_F {
    D2D_RECT_F {
        left: coordinate(bounds.x),
        top: coordinate(bounds.y),
        right: coordinate(bounds.right()),
        bottom: coordinate(bounds.bottom()),
    }
}

fn rounded(rect: D2D_RECT_F, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect,
        radiusX: radius,
        radiusY: radius,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_channels_scale_to_one() {
        let color = color_f(Color::argb(0x80FF_0000));

        assert_eq!(
            (color.r, color.g, color.b, color.a),
            (1.0, 0.0, 0.0, 128.0 / 255.0)
        );
    }

    #[test]
    fn rectangle_spans_bounds() {
        let rect = rect_f(Bounds {
            x: 4,
            y: 8,
            width: 100,
            height: 32,
        });

        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (4.0, 8.0, 104.0, 40.0)
        );
    }
}
