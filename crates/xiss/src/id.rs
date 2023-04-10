use smol_str::SmolStr;
use xiss_map::IdKind;

#[derive(Debug)]
pub struct Id {
    pub kind: IdKind,
    /// Index in [crate::CssMap]
    pub module_index: u32,
    pub local_id: SmolStr,
    pub global_id: SmolStr,
}

impl Id {
    pub fn new(kind: IdKind, module_index: u32, local_id: SmolStr, global_id: SmolStr) -> Self {
        Self {
            kind,
            module_index,
            local_id,
            global_id,
        }
    }
}
