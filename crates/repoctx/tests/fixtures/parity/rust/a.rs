pub struct Widget {
    pub size: u32,
}

impl Widget {
    pub fn build() -> Self {
        // GhostInComment is mentioned only in this comment.
        let _label = "GhostInString";
        Widget { size: 0 }
    }
}

pub fn helper() -> u32 {
    42
}

pub const MAX_SIZE: u32 = 100;
