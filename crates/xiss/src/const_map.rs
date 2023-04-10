use std::path::Path;

use rustc_hash::FxHashMap;
use swc_atoms::JsWord;
use swc_css::{
    ast::{ComponentValue, Declaration, DeclarationName},
    visit::{Visit, VisitWith},
};

use crate::css::process_css;

struct ConstDecl<'a> {
    index: &'a mut FxHashMap<JsWord, Vec<ComponentValue>>,
}

impl Visit for ConstDecl<'_> {
    fn visit_declaration(&mut self, decl: &Declaration) {
        if let DeclarationName::DashedIdent(ident) = &decl.name {
            self.index.insert(ident.value.clone(), decl.value.clone());
        }
    }
}

pub fn extract_const_values(
    path: &Path,
    contents: String,
) -> Result<FxHashMap<JsWord, Vec<ComponentValue>>, String> {
    process_css(path, contents, |handler, stylesheet| {
        let mut values = FxHashMap::default();
        stylesheet.visit_with(&mut ConstDecl { index: &mut values });
        if handler.has_errors() {
            None
        } else {
            Some(values)
        }
    })
}
