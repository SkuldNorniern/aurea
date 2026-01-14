use crate::render::Rect;

pub struct DamageRegion {
    rects: Vec<Rect>,
    max_rects: usize,
    union: Option<Rect>,
}

impl DamageRegion {
    pub fn new(max_rects: usize) -> Self {
        Self {
            rects: Vec::with_capacity(max_rects),
            max_rects,
            union: None,
        }
    }

    pub fn add(&mut self, rect: Rect) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        if self.rects.len() < self.max_rects {
            self.rects.push(rect);
        }

        self.union = match self.union {
            Some(u) => Some(union_rect(u, rect)),
            None => Some(rect),
        };
    }

    pub fn add_all(&mut self) {
        self.rects.clear();
        // Note: union is set when we have dimensions, but for now we clear it
        // The caller should set the full canvas rect explicitly
        self.union = None;
    }

    pub fn set_full(&mut self, width: f32, height: f32) {
        self.rects.clear();
        self.union = Some(Rect::new(0.0, 0.0, width, height));
    }

    pub fn take(&mut self) -> Option<Rect> {
        let result = self.union.take();
        self.rects.clear();
        result
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.union = None;
    }

    pub fn is_empty(&self) -> bool {
        self.union.is_none()
    }

    pub fn union(&self) -> Option<Rect> {
        self.union
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);

    Rect::new(left, top, right - left, bottom - top)
}
