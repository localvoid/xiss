use std::fmt;

use smol_str::SmolStr;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum IdKind {
    Class,
    Var,
    Keyframes,
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdKind::Class => f.write_str("class"),
            IdKind::Var => f.write_str("var"),
            IdKind::Keyframes => f.write_str("keyframes"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Id {
    pub kind: IdKind,
    pub module_id: SmolStr,
    pub local_id: SmolStr,
    pub global_id: SmolStr,
}

impl Id {
    pub fn new(kind: IdKind, module_id: SmolStr, local_id: SmolStr, global_id: SmolStr) -> Self {
        Self {
            kind,
            module_id,
            local_id,
            global_id,
        }
    }
}

impl<'a> fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},{},{}\n",
            self.module_id, self.kind, self.local_id, self.global_id,
        )
    }
}
