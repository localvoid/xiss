use std::{
    collections::hash_map::Entry,
    io::{self, Write},
    rc::Rc,
};

use regex::Regex;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use xiss_map::IdKind;

use crate::{global_id::IdSet, id::Id};

#[derive(Debug, thiserror::Error)]
pub enum CssMapError {
    #[error(transparent)]
    IOError(#[from] io::Error),
    #[error(transparent)]
    ParserError(#[from] xiss_map::parser::Error),
    #[error("Invalid exclude rule: {0}")]
    InvalidExcludeRule(regex::Error),
    #[error("Duplicate entry: {0},{1},{2}")]
    DuplicateEntry(IdKind, SmolStr, SmolStr),
}

pub struct CssMapModule {
    pub id: SmolStr,
    pub index: u32,
    pub classes: FxHashMap<SmolStr, Rc<Id>>,
    pub vars: FxHashMap<SmolStr, Rc<Id>>,
    pub keyframes: FxHashMap<SmolStr, Rc<Id>>,
}

impl CssMapModule {
    pub fn new(id: SmolStr, index: u32) -> Self {
        Self {
            id,
            index,
            classes: FxHashMap::default(),
            vars: FxHashMap::default(),
            keyframes: FxHashMap::default(),
        }
    }
}

/// [CssMap] is used to create unique identifiers for class names, variables
/// and keyframes.
pub struct CssMap {
    index: FxHashMap<SmolStr, u32>,
    pub modules: Vec<Box<CssMapModule>>,
    classes: IdSet,
    vars: IdSet,
    keyframes: IdSet,
    new_ids_buf: String,
}

impl CssMap {
    pub fn new(
        exclude_class: &Vec<String>,
        exclude_var: &Vec<String>,
        exclude_keyframes: &Vec<String>,
    ) -> Result<Self, CssMapError> {
        Ok(Self {
            index: FxHashMap::default(),
            modules: Vec::new(),
            classes: IdSet::new(0, build_exclude(exclude_class)?),
            vars: IdSet::new(0, build_exclude(exclude_var)?),
            keyframes: IdSet::new(0, build_exclude(exclude_keyframes)?),
            new_ids_buf: String::new(),
        })
    }

    /// Imports css map from [io::BufRead].
    pub fn import<R: io::BufRead>(&mut self, reader: &mut R) -> Result<(), CssMapError> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        let mut parser = xiss_map::parser::Parser::new(&buf);
        let mut module_index = 0;

        while let Some((kind, module_id, local_id, global_id)) = parser.next_id()? {
            if let Some(module_id) = module_id {
                let i = match self.index.entry(module_id.into()) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        let index = self.modules.len() as u32;
                        self.modules
                            .push(Box::new(CssMapModule::new(entry.key().clone(), index)));
                        *entry.insert(index)
                    }
                };
                module_index = i;
            }
            if let Some(module) = self.modules.get_mut(module_index as usize) {
                let id = Rc::new(Id::new(
                    kind,
                    module.index,
                    local_id.into(),
                    global_id.into(),
                ));
                match kind {
                    IdKind::Class => {
                        insert_id(&module.id, &mut module.classes, &mut self.classes, id)?
                    }
                    IdKind::Var => insert_id(&module.id, &mut module.vars, &mut self.vars, id)?,
                    IdKind::Keyframes => {
                        insert_id(&module.id, &mut module.keyframes, &mut self.keyframes, id)?
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns [Id] if it exists or creates a new one.
    pub fn get_id(&mut self, module_index: u32, id_kind: IdKind, local_id: &str) -> Rc<Id> {
        let module = &mut self.modules[module_index as usize];
        let (id_char, id_set, map) = match id_kind {
            IdKind::Class => ('C', &mut self.classes, &mut module.classes),
            IdKind::Var => ('V', &mut self.vars, &mut module.vars),
            IdKind::Keyframes => ('K', &mut self.keyframes, &mut module.keyframes),
        };
        if let Some(id) = map.get(local_id) {
            id.clone()
        } else {
            let global_id = id_set.next_id();

            let buf = &mut self.new_ids_buf;
            buf.reserve(5 + module.id.len() + local_id.len() + global_id.len());
            buf.push(id_char);
            buf.push(',');
            buf.push_str(&module.id);
            buf.push(',');
            buf.push_str(local_id);
            buf.push(',');
            buf.push_str(&global_id);
            buf.push('\n');

            let id = Rc::new(Id::new(
                IdKind::Class,
                module_index,
                local_id.into(),
                global_id,
            ));
            map.insert(local_id.into(), id.clone());

            id
        }
    }

    /// Returns module index if the module exists, otherwise creates a new one
    /// and returns its index.
    pub fn get_module_index(&mut self, module_name: &str) -> u32 {
        get_module_index(&mut self.index, &mut self.modules, module_name)
    }

    /// Writes new ids into the [output].
    pub fn flush_new_ids<W: Write>(&mut self, output: &mut W) -> Result<(), io::Error> {
        output.write(self.new_ids_buf.as_bytes())?;
        output.flush()?;
        self.new_ids_buf.clear();
        Ok(())
    }
}

/// Builds a vector of [Regex] objects from a vector of regex strings.
///
/// TODO: evaluate the possibility to build one [Regex] from many regex strings
/// by joining them with `|` operator.
fn build_exclude(exclude: &Vec<String>) -> Result<Vec<Regex>, CssMapError> {
    let mut result = Vec::with_capacity(exclude.len());
    for s in exclude {
        let r = match Regex::new(s) {
            Ok(r) => r,
            Err(err) => return Err(CssMapError::InvalidExcludeRule(err)),
        };
        result.push(r);
    }
    Ok(result)
}

/// Returns module index if the module exists, otherwise creates a new one and
/// returns its index,
fn get_module_index<'a>(
    modules_index: &mut FxHashMap<SmolStr, u32>,
    modules: &'a mut Vec<Box<CssMapModule>>,
    module_name: &str,
) -> u32 {
    if let Some(module_id) = modules_index.get(module_name) {
        *module_id
    } else {
        let module_id = modules.len() as u32;
        modules_index.insert(module_name.into(), module_id);
        modules.push(Box::new(CssMapModule::new(module_name.into(), module_id)));
        module_id
    }
}

fn insert_id(
    module_id: &str,
    map: &mut FxHashMap<SmolStr, Rc<Id>>,
    id_set: &mut IdSet,
    id: Rc<Id>,
) -> Result<(), CssMapError> {
    if let Entry::Vacant(v) = map.entry(id.local_id.clone()) {
        id_set.add(&id.global_id);
        v.insert(id);
        Ok(())
    } else {
        Err(CssMapError::DuplicateEntry(
            id.kind,
            module_id.into(),
            id.local_id.clone(),
        ))
    }
}
