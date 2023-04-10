use std::{
    collections::hash_map::Entry,
    error, fmt,
    io::{self},
};

use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::{parser, Id, IdKind};

#[derive(Debug, Clone, Copy)]
pub struct ModuleIndex(usize);

#[derive(Debug)]
pub struct Map {
    index: FxHashMap<SmolStr, ModuleIndex>,
    items: Vec<Box<Module>>,
}

impl Map {
    pub fn new() -> Self {
        Self {
            index: FxHashMap::default(),
            items: Vec::new(),
        }
    }

    pub fn get_by_index(&self, index: ModuleIndex) -> Option<&Module> {
        self.items.get(index.0).map(|m| &**m)
    }

    pub fn get_mut_by_index(&mut self, index: ModuleIndex) -> Option<&mut Module> {
        self.items.get_mut(index.0).map(|m| &mut **m)
    }

    pub fn get_by_id(&self, id: &str) -> Option<&Module> {
        if let Some(index) = self.index.get(id) {
            self.get_by_index(*index)
        } else {
            None
        }
    }

    pub fn get_mut_by_id(&mut self, id: &str) -> Option<&mut Module> {
        if let Some(index) = self.index.get(id) {
            self.get_mut_by_index(*index)
        } else {
            None
        }
    }

    /// Imports css map from [io::BufRead].
    pub fn import<R: io::BufRead>(&mut self, reader: &mut R) -> Result<(), Error> {
        let mut contents = String::new();
        reader.read_to_string(&mut contents)?;
        let mut p = parser::Parser::new(&contents);
        let mut module_index = ModuleIndex(0);
        while let Some((kind, module_id, local_id, global_id)) = p.next_id()? {
            if let Some(module_id) = module_id {
                let i = match self.index.entry(SmolStr::from(module_id).clone()) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        let index = ModuleIndex(self.items.len());
                        self.items
                            .push(Box::new(Module::new(entry.key().clone(), index)));
                        *entry.insert(index)
                    }
                };
                module_index = i;
            }
            if let Some(module) = self.get_mut_by_index(module_index) {
                let id = Id::new(kind, module.id.clone(), local_id.into(), global_id.into());
                match kind {
                    IdKind::Class => insert_id(&mut module.classes, id)?,
                    IdKind::Var => insert_id(&mut module.vars, id)?,
                    IdKind::Keyframes => insert_id(&mut module.keyframes, id)?,
                }
            }
        }
        Ok(())
    }
}

fn insert_id<'a>(map: &mut FxHashMap<SmolStr, Id>, id: Id) -> Result<(), Error> {
    match map.entry(id.local_id.clone()) {
        Entry::Occupied(..) => Err(Error::DuplicateEntry(id)),
        Entry::Vacant(v) => {
            v.insert(id);
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct Module {
    pub id: SmolStr,
    pub index: ModuleIndex,
    pub classes: FxHashMap<SmolStr, Id>,
    pub vars: FxHashMap<SmolStr, Id>,
    pub keyframes: FxHashMap<SmolStr, Id>,
}

impl Module {
    pub fn new(id: SmolStr, index: ModuleIndex) -> Self {
        Self {
            id,
            index,
            classes: FxHashMap::default(),
            vars: FxHashMap::default(),
            keyframes: FxHashMap::default(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    IOError(io::Error),
    ParserError(parser::Error),
    DuplicateEntry(Id),
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::IOError(value)
    }
}

impl From<parser::Error> for Error {
    fn from(value: parser::Error) -> Self {
        Error::ParserError(value)
    }
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::IOError(err) => err.fmt(f),
            Error::ParserError(err) => err.fmt(f),
            Error::DuplicateEntry(id) => write!(f, "Duplicate entry: '{}'", id),
        }
    }
}
