//! What a GPU backend draws itself, what a frame asks of it, and what happens when the two do not
//! meet.

/// Kinds of drawing, as a set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuFeatures {
    /// Gradient fills.
    pub gradients: bool,
    /// Images.
    pub images: bool,
    /// Text glyphs.
    pub text: bool,
    /// Rectangular clips, applied as scissors.
    pub clips: bool,
    /// Clips to any path shape, such as a rounded card or an SVG mask.
    pub path_clips: bool,
    /// Rounded rects and shadows.
    pub rounded_rects: bool,
    /// Arbitrary filled and stroked paths, and the older text commands, which only the CPU
    /// rasterizer draws.
    pub paths: bool,
    /// Blend modes other than normal alpha blending.
    pub blend_modes: bool,
    /// Stroked circles, which the instanced circle path cannot draw.
    pub stroked_circles: bool,
    /// App-made GPU textures.
    pub textures: bool,
}

impl GpuFeatures {
    /// Nothing at all.
    pub const NONE: Self = Self {
        gradients: false,
        images: false,
        text: false,
        clips: false,
        path_clips: false,
        rounded_rects: false,
        paths: false,
        blend_modes: false,
        stroked_circles: false,
        textures: false,
    };

    /// Everything the CPU rasterizer draws.
    pub const ALL: Self = Self {
        gradients: true,
        images: true,
        text: true,
        clips: true,
        path_clips: true,
        rounded_rects: true,
        paths: true,
        blend_modes: true,
        stroked_circles: true,
        textures: false,
    };

    /// What `needed` asks for that this set does not have.
    pub const fn missing(self, needed: Self) -> Self {
        Self {
            gradients: needed.gradients && !self.gradients,
            images: needed.images && !self.images,
            text: needed.text && !self.text,
            clips: needed.clips && !self.clips,
            path_clips: needed.path_clips && !self.path_clips,
            rounded_rects: needed.rounded_rects && !self.rounded_rects,
            paths: needed.paths && !self.paths,
            blend_modes: needed.blend_modes && !self.blend_modes,
            stroked_circles: needed.stroked_circles && !self.stroked_circles,
            textures: needed.textures && !self.textures,
        }
    }

    /// Both sets together.
    pub const fn union(self, other: Self) -> Self {
        Self {
            gradients: self.gradients || other.gradients,
            images: self.images || other.images,
            text: self.text || other.text,
            clips: self.clips || other.clips,
            path_clips: self.path_clips || other.path_clips,
            rounded_rects: self.rounded_rects || other.rounded_rects,
            paths: self.paths || other.paths,
            blend_modes: self.blend_modes || other.blend_modes,
            stroked_circles: self.stroked_circles || other.stroked_circles,
            textures: self.textures || other.textures,
        }
    }

    /// True when no kind of drawing is set.
    pub const fn is_empty(self) -> bool {
        !(self.gradients
            || self.images
            || self.text
            || self.clips
            || self.path_clips
            || self.rounded_rects
            || self.paths
            || self.blend_modes
            || self.stroked_circles
            || self.textures)
    }
}

/// What a GPU renderer does with a frame its backend cannot draw all of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fallback {
    /// Rasterize the whole frame on the CPU and present the pixels through the GPU surface.
    #[default]
    CpuFrame,
    /// Draw what the backend can and leave the rest out, saying once what was missing.
    Partial,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_what_is_asked_and_not_had() {
        let had = GpuFeatures {
            images: true,
            clips: true,
            ..GpuFeatures::NONE
        };
        let asked = GpuFeatures {
            images: true,
            paths: true,
            ..GpuFeatures::NONE
        };
        let missing = had.missing(asked);
        assert_eq!(
            missing,
            GpuFeatures {
                paths: true,
                ..GpuFeatures::NONE
            }
        );
        assert!(GpuFeatures::ALL.missing(asked).is_empty());
        assert!(had.missing(GpuFeatures::NONE).is_empty());
    }

    #[test]
    fn union_collects_both() {
        let a = GpuFeatures {
            text: true,
            ..GpuFeatures::NONE
        };
        let b = GpuFeatures {
            gradients: true,
            ..GpuFeatures::NONE
        };
        let both = a.union(b);
        assert!(both.text && both.gradients && !both.paths);
    }
}
