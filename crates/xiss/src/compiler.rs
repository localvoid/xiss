use std::{fmt::Write, path::Path, rc::Rc};

use phf::phf_map;
use rustc_hash::FxHashMap;
use swc_atoms::JsWord;
use swc_common::{errors::HANDLER, util::take::Take, Span, DUMMY_SP};
use swc_css::{
    ast::*,
    codegen::{writer::basic::BasicCssWriter, CodeGenerator, Emit},
    visit::{VisitMut, VisitMutWith},
};
use tracing::error;
use xiss_map::IdKind;

use crate::{
    class_map::{ClassMap, ClassMapOutput, ClassMapState},
    css::process_css,
    css_map::{CssMap, CssMapModule},
    id::Id,
};

pub fn compile<P: AsRef<Path>>(
    path: P,
    contents: String,
    css_map: &mut CssMap,
    const_map: &FxHashMap<JsWord, Vec<ComponentValue>>,
    module_id: &str,
    class_map_output: ClassMapOutput,
) -> Result<CompilationArtifact, String> {
    process_css(path, contents, |handler, stylesheet| {
        let mut css = String::new();
        let mut js = String::new();
        let mut ts = String::new();

        stylesheet.visit_mut_with(&mut UpdateConstValues { const_map });
        if !handler.has_errors() {
            let module_index = css_map.get_module_index(module_id);
            let mut module_compiler = ModuleCompiler::new(css_map, module_index);
            stylesheet.visit_mut_with(&mut module_compiler);
            if !handler.has_errors() {
                if module_compiler.has_keyframes {
                    stylesheet.visit_mut_with(&mut TransformAnimationNames::new(
                        &module_compiler.scope.keyframes,
                    ));
                }

                if let Err(err) = emit_js(&mut js, &module_compiler.class_maps, class_map_output) {
                    handler.err(&format!("Failed to emit js: {}", err));
                }

                let mut classes: Vec<(&JsWord, &Rc<Id>)> =
                    module_compiler.scope.classes.iter().collect();
                classes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                let mut vars: Vec<(&JsWord, &Rc<Id>)> = module_compiler.scope.vars.iter().collect();
                vars.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                let mut keyframes: Vec<(&JsWord, &Rc<Id>)> =
                    module_compiler.scope.keyframes.iter().collect();
                keyframes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                if let Err(err) = emit_ts(
                    &mut ts,
                    &classes,
                    &vars,
                    &keyframes,
                    module_index,
                    &module_compiler.class_maps,
                    &module_compiler.scope.css_map.modules,
                ) {
                    handler.err(&format!("Failed to emit types: {}", err));
                }

                let writer = BasicCssWriter::new(&mut css, None, Default::default());
                let mut gen = CodeGenerator::new(writer, Default::default());
                if let Err(err) = gen.emit(stylesheet) {
                    handler.err(&format!("Failed to emit css: {}", err));
                };
            }
        }

        if handler.has_errors() {
            None
        } else {
            Some(CompilationArtifact { css, js, ts })
        }
    })
}

static ID_KIND: phf::Map<&'static str, IdKind> = phf_map! {
    "class" => IdKind::Class,
    "var" => IdKind::Var,
    "keyframes" => IdKind::Keyframes,
};

#[derive(Debug, Clone)]
pub struct ExternSymbol {
    pub kind: IdKind,
    pub local_id: JsWord,
    pub imported_id: JsWord,
    pub module_id: JsWord,
}

#[derive(Debug)]
pub struct ParserError {
    span: Span,
    kind: ParserErrorKind,
}

impl ParserError {
    fn new(span: Span, kind: ParserErrorKind) -> Self {
        Self { span, kind }
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
enum ParserErrorKind {
    #[error("Expected whitespace")]
    ExpectedWhitespace,
    #[error("Expected keyword '{0}'")]
    ExpectedKeyword(String),
    #[error("Expected identifier")]
    ExpectedIdentifier,
    #[error("Expected valid JS identifier `[a-zA-Z_][a-zA-Z0-9_]*`")]
    ExpectedValidJSIdentifier,
    #[error("Expected string")]
    ExpectedString,
    #[error("Expected semicolon")]
    ExpectedSemicolon,
    #[error("Expected colon")]
    ExpectedColon,
    #[error("Expected prelude")]
    ExpectedPrelude,
    #[error("Expected block")]
    ExpectedBlock,
    #[error("Unknown token")]
    UnknownToken,
    #[error("Unknown at rule")]
    UnknownAtRule,
    #[error("Unexpected component value")]
    UnexpectedComponentValue,
}

type ParserResult<T> = Result<T, ParserError>;

fn join_words(words: &[JsWord]) -> String {
    let mut result = String::new();
    let mut iter = words.iter();
    if let Some(word) = iter.next() {
        result.push_str(word);
        for word in iter {
            result.push(' ');
            result.push_str(word);
        }
    }
    result
}

fn expect_ident(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
    is_js_ident: bool,
) -> ParserResult<JsWord> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::Ident { value, .. } => {
                    iter.next();
                    if is_js_ident && !is_valid_js_ident(&value) {
                        Err(ParserError::new(
                            token_and_span.span,
                            ParserErrorKind::ExpectedValidJSIdentifier,
                        ))
                    } else {
                        Ok(value.clone())
                    }
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedIdentifier,
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedIdentifier,
        ))
    }
}

fn expect_keyword(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    keyword: &'static str,
    span: &Span,
) -> ParserResult<()> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::Ident { value, .. } if &*value == keyword => {
                    iter.next();
                    Ok(())
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedKeyword(keyword.into()),
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedKeyword(keyword.into()),
        ))
    }
}

fn expect_colon(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
) -> ParserResult<()> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::Colon => {
                    iter.next();
                    Ok(())
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedColon,
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedColon,
        ))
    }
}

fn expect_semi(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
) -> ParserResult<()> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::Semi => {
                    iter.next();
                    Ok(())
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedSemicolon,
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedColon,
        ))
    }
}

fn expect_string(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
) -> ParserResult<JsWord> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::String { value, .. } => {
                    iter.next();
                    Ok(value.clone())
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedString,
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedString,
        ))
    }
}

fn skip_whitespace(iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>) {
    if let Some(ComponentValue::PreservedToken(token_and_span)) = iter.peek() {
        if let Token::WhiteSpace { .. } = &token_and_span.token {
            iter.next();
        }
    }
}

fn expect_whitespace(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
) -> ParserResult<()> {
    if let Some(next) = iter.peek() {
        match next {
            ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                Token::WhiteSpace { .. } => {
                    iter.next();
                    Ok(())
                }
                _ => Err(ParserError::new(
                    token_and_span.span,
                    ParserErrorKind::ExpectedWhitespace,
                )),
            },
            _ => Err(ParserError::new(
                DUMMY_SP,
                ParserErrorKind::UnexpectedComponentValue,
            )),
        }
    } else {
        Err(ParserError::new(
            span.shrink_to_hi(),
            ParserErrorKind::ExpectedWhitespace,
        ))
    }
}

fn expect_identifier_list(
    iter: &mut std::iter::Peekable<std::slice::Iter<ComponentValue>>,
    span: &Span,
    is_js_ident: bool,
) -> ParserResult<Vec<JsWord>> {
    let ident = expect_ident(iter, span, is_js_ident)?;
    let mut result = vec![ident];

    while expect_whitespace(iter, span).is_ok() {
        if let Ok(ident) = expect_ident(iter, span, is_js_ident) {
            result.push(ident);
        } else {
            break;
        }
    }

    Ok(result)
}

fn parse_at_extern(at_rule: &AtRule) -> ParserResult<ExternSymbol> {
    // (Ident(class) | Ident(var) | Ident(keyframes)) Ident (Ident(as) Ident)?
    // Ident(from) String
    if let Some(prelude) = &at_rule.prelude {
        match &**prelude {
            AtRulePrelude::ListOfComponentValues(values) => {
                let mut iter = values.children.iter().peekable();
                expect_whitespace(&mut iter, &values.span)?;
                let symbol_kind_ident = expect_ident(&mut iter, &values.span, false)?;
                if let Some(&kind) = ID_KIND.get(&symbol_kind_ident) {
                    expect_whitespace(&mut iter, &values.span)?;

                    let mut imported_iter = iter.clone();
                    let imported = expect_ident(&mut iter, &values.span, false)?;
                    let mut local = imported.clone();
                    expect_whitespace(&mut iter, &values.span)?;

                    if expect_keyword(&mut iter, "as", &values.span).is_ok() {
                        expect_whitespace(&mut iter, &values.span)?;
                        local = expect_ident(&mut iter, &values.span, true)?;
                        expect_whitespace(&mut iter, &values.span)?;
                    } else {
                        if !is_valid_js_ident(&local) {
                            if let ComponentValue::PreservedToken(token) =
                                imported_iter.next().unwrap()
                            {
                                return Err(ParserError::new(
                                    token.span,
                                    ParserErrorKind::ExpectedValidJSIdentifier,
                                ));
                            }
                        }
                    }

                    expect_keyword(&mut iter, "from", &values.span)?;
                    expect_whitespace(&mut iter, &values.span)?;
                    let module_id = expect_string(&mut iter, &values.span)?;

                    Ok(ExternSymbol {
                        kind,
                        local_id: local,
                        imported_id: imported,
                        module_id,
                    })
                } else {
                    Err(ParserError::new(
                        DUMMY_SP,
                        ParserErrorKind::ExpectedIdentifier,
                    ))
                }
            }
            _ => todo!("invalid prelude"),
        }
    } else {
        todo!();
    }
}

fn parse_at_class_map(at_rule: &AtRule, scope: &mut ModuleScope) -> ParserResult<ClassMap> {
    // WhiteSpace+ Ident WhiteSpace*
    let name = if let Some(prelude) = &at_rule.prelude {
        if let AtRulePrelude::ListOfComponentValues(values) = &**prelude {
            let mut iter = values.children.iter().peekable();
            expect_whitespace(&mut iter, &values.span)?;
            let ident = expect_ident(&mut iter, &values.span, true)?;
            skip_whitespace(&mut iter);
            if let Some(next) = iter.next() {
                if let ComponentValue::PreservedToken(token_and_span) = next {
                    return Err(ParserError::new(
                        token_and_span.span,
                        ParserErrorKind::UnknownToken,
                    ));
                } else {
                    return Err(ParserError::new(DUMMY_SP, ParserErrorKind::UnknownToken));
                }
            } else {
                ident
            }
        } else {
            return Err(ParserError::new(
                at_rule.span,
                ParserErrorKind::ExpectedPrelude,
            ));
        }
    } else {
        return Err(ParserError::new(
            at_rule.span,
            ParserErrorKind::ExpectedPrelude,
        ));
    };

    // (
    //   WhiteSpace*
    //   (
    //     (@static (WhiteSpace+ Ident)+) |
    //     (@exclude (WhiteSpace+ Ident)+) |
    //     (Ident WhiteSpace* Colon WhiteSpace* Ident)
    //   )
    //   WhiteSpace* Semi
    // )*
    if let Some(block) = &at_rule.block {
        let mut state_index: u32 = 0;
        let mut states = vec![];
        let mut states_map_index = FxHashMap::default();
        let mut static_classes = vec![];
        let mut exclude_constraints = vec![];
        let mut iter = block.value.iter().peekable();
        while let Some(next) = iter.next() {
            match next {
                ComponentValue::PreservedToken(token_and_span) => match &token_and_span.token {
                    Token::AtKeyword { value, .. } => match &**value {
                        "static" => {
                            expect_whitespace(&mut iter, &block.span)?;
                            static_classes = expect_identifier_list(&mut iter, &block.span, true)?;
                            expect_semi(&mut iter, &block.span)?;
                        }
                        "exclude" => {
                            expect_whitespace(&mut iter, &block.span)?;
                            let mut exclude_mask: usize = 0;
                            for ident in expect_identifier_list(&mut iter, &block.span, true)? {
                                if let Some(index) = states_map_index.get(&ident) {
                                    exclude_mask |= 1usize << *index;
                                } else {
                                    todo!("Invalid rule");
                                }
                            }
                            exclude_constraints.push(exclude_mask);
                            expect_semi(&mut iter, &block.span)?;
                        }
                        _ => {
                            return Err(ParserError::new(
                                token_and_span.span,
                                ParserErrorKind::UnknownAtRule,
                            ));
                        }
                    },
                    Token::Ident {
                        value: state_name, ..
                    } => {
                        skip_whitespace(&mut iter);
                        expect_colon(&mut iter, &block.span)?;
                        skip_whitespace(&mut iter);
                        let mut classes = expect_identifier_list(&mut iter, &block.span, true)?;
                        expect_semi(&mut iter, &block.span)?;

                        // resolve class name identifiers
                        for c in classes.iter_mut() {
                            *c = (&scope.get_id(IdKind::Class, c).global_id[..]).into();
                        }

                        states_map_index.insert(state_name.clone(), state_index);
                        states.push(ClassMapState::new(state_name.clone(), join_words(&classes)));
                        state_index += 1;
                    }
                    Token::WhiteSpace { .. } => {}
                    _ => {
                        return Err(ParserError::new(
                            token_and_span.span,
                            ParserErrorKind::UnknownToken,
                        ));
                    }
                },
                _ => {
                    return Err(ParserError::new(
                        block.span,
                        ParserErrorKind::UnexpectedComponentValue,
                    ));
                }
            }
        }
        // resolve static classes identifiers
        for c in static_classes.iter_mut() {
            *c = (&scope.get_id(IdKind::Class, c).global_id[..]).into();
        }

        Ok(ClassMap::new(
            name,
            join_words(&static_classes),
            states,
            exclude_constraints,
        ))
    } else {
        Err(ParserError::new(
            at_rule.span,
            ParserErrorKind::ExpectedBlock,
        ))
    }
}

struct TransformAnimationNames<'a> {
    scope: &'a FxHashMap<JsWord, Rc<Id>>,
}

impl<'a> TransformAnimationNames<'a> {
    fn new(scope: &'a FxHashMap<JsWord, Rc<Id>>) -> Self {
        Self { scope }
    }
}

impl VisitMut for TransformAnimationNames<'_> {
    fn visit_mut_declaration(&mut self, decl: &mut Declaration) {
        decl.visit_mut_children_with(self);
        if let DeclarationName::Ident(ident) = &mut decl.name {
            if &ident.value == "animation" {
                for v in decl.value.iter_mut() {
                    if let ComponentValue::Ident(ident) = v {
                        if let Some(id) = self.scope.get(&ident.value) {
                            if id.kind == IdKind::Keyframes {
                                ident.value = (&id.global_id[..]).into();
                            }
                        }
                    }
                }
            }
        }
    }
}

struct ModuleScope<'a> {
    css_map: &'a mut CssMap,
    module_index: u32,
    classes: FxHashMap<JsWord, Rc<Id>>,
    vars: FxHashMap<JsWord, Rc<Id>>,
    keyframes: FxHashMap<JsWord, Rc<Id>>,
}

impl<'a> ModuleScope<'a> {
    fn new(css_map: &'a mut CssMap, module_index: u32) -> Self {
        Self {
            css_map,
            module_index,
            classes: FxHashMap::default(),
            vars: FxHashMap::default(),
            keyframes: FxHashMap::default(),
        }
    }

    fn add_extern(
        &mut self,
        module_id: &JsWord,
        id_kind: IdKind,
        local_id: &JsWord,
        imported_id: &JsWord,
    ) -> Rc<Id> {
        let map = match id_kind {
            IdKind::Class => &mut self.classes,
            IdKind::Var => &mut self.vars,
            IdKind::Keyframes => &mut self.keyframes,
        };
        if let Some(id) = map.get(local_id) {
            id.clone()
        } else {
            let module_index = self.css_map.get_module_index(module_id);
            let id = self.css_map.get_id(module_index, id_kind, imported_id);
            map.insert(local_id.clone(), id.clone());
            id
        }
    }

    fn get_id(&mut self, id_kind: IdKind, local_id: &JsWord) -> Rc<Id> {
        let map = match id_kind {
            IdKind::Class => &mut self.classes,
            IdKind::Var => &mut self.vars,
            IdKind::Keyframes => &mut self.keyframes,
        };
        if let Some(id) = map.get(local_id) {
            id.clone()
        } else {
            let id = self.css_map.get_id(self.module_index, id_kind, local_id);
            map.insert(local_id.clone(), id.clone());
            id
        }
    }
}

struct UpdateConstValues<'a> {
    const_map: &'a FxHashMap<JsWord, Vec<ComponentValue>>,
}

impl VisitMut for UpdateConstValues<'_> {
    fn visit_mut_declaration(&mut self, decl: &mut Declaration) {
        let mut has_const = false;
        for v in decl.value.iter() {
            if let ComponentValue::Function(func) = v {
                if let FunctionName::Ident(ident) = &func.name {
                    if &ident.value == "const" {
                        has_const = true;
                    }
                }
            }
        }

        if has_const {
            let mut r = Vec::with_capacity(decl.value.len());
            for v in decl.value.iter() {
                if let ComponentValue::Function(func) = v {
                    if let FunctionName::Ident(ident) = &func.name {
                        if &ident.value == "const" {
                            if let Some(ComponentValue::DashedIdent(ident)) = func.value.get(0) {
                                if let Some(value) = self.const_map.get(&ident.value) {
                                    r.extend(value.clone());
                                } else {
                                    HANDLER.with(|handler| {
                                        handler
                                            .struct_span_err(
                                                ident.span,
                                                &format!(
                                                    "Cannot find a const value '{}'",
                                                    ident.value
                                                ),
                                            )
                                            .emit();
                                    });
                                }
                            } else {
                                HANDLER.with(|handler| {
                                    handler
                                        .struct_span_err(
                                            func.span,
                                            "Invalid function argument, const function should \
                                             have a dashed identifier, e.g. '--CONST-VAR'",
                                        )
                                        .emit();
                                });
                            }
                            continue;
                        }
                    }
                }
                r.push(v.clone());
            }
            decl.value = r;
        }
    }
}

struct ModuleCompiler<'a> {
    scope: ModuleScope<'a>,
    class_maps: Vec<ClassMap>,
    has_keyframes: bool,
}

impl<'a> ModuleCompiler<'a> {
    fn new(css_map: &'a mut CssMap, module_id: u32) -> Self {
        Self {
            scope: ModuleScope::new(css_map, module_id),
            class_maps: Vec::new(),
            has_keyframes: false,
        }
    }
}

impl VisitMut for ModuleCompiler<'_> {
    fn visit_mut_class_selector(&mut self, selector: &mut ClassSelector) {
        let id = self.scope.get_id(IdKind::Class, &selector.text.value);
        selector.text.value = (&id.global_id[..]).into();
    }

    fn visit_mut_dashed_ident(&mut self, ident: &mut DashedIdent) {
        let id = self.scope.get_id(IdKind::Var, &ident.value);
        ident.value = (&id.global_id[..]).into();
    }

    fn visit_mut_keyframes_name(&mut self, name: &mut KeyframesName) {
        // Ignore `@keyframes "string identifier" {...}`
        if let KeyframesName::CustomIdent(ident) = name {
            let id = self.scope.get_id(IdKind::Keyframes, &ident.value);
            ident.value = (&id.global_id[..]).into();
            self.has_keyframes = true;
        }
    }

    fn visit_mut_rules(&mut self, rules: &mut Vec<Rule>) {
        let mut new_rules = Vec::with_capacity(rules.len());

        for rule in rules.iter_mut() {
            match rule {
                Rule::AtRule(at_rule) => {
                    if &at_rule.name == "extern" {
                        match parse_at_extern(at_rule) {
                            Ok(ext) => {
                                self.scope.add_extern(
                                    &ext.module_id,
                                    ext.kind,
                                    &ext.local_id,
                                    &ext.imported_id,
                                );
                            }
                            Err(err) => {
                                HANDLER.with(|handler| {
                                    handler.struct_span_err(err.span, &err.to_string()).emit();
                                });
                            }
                        }
                    } else if &at_rule.name == "classmap" {
                        match parse_at_class_map(at_rule, &mut self.scope) {
                            Ok(class_map) => {
                                let states_num = class_map.states.len();
                                if states_num >= 2 && states_num <= 8 {
                                    self.class_maps.push(class_map);
                                } else {
                                    HANDLER.with(|handler| {
                                        handler
                                            .struct_span_err(
                                                at_rule.span,
                                                &format!(
                                                    "class map should have at least 2 states and \
                                                     no more than 8 states but got {} states",
                                                    states_num
                                                ),
                                            )
                                            .emit();
                                    });
                                }
                            }
                            Err(err) => {
                                HANDLER.with(|handler| {
                                    handler.struct_span_err(err.span, &err.to_string()).emit();
                                });
                            }
                        }
                    } else {
                        rule.visit_mut_children_with(self);
                        new_rules.push(rule.take());
                    }
                }
                _ => {
                    rule.visit_mut_children_with(self);
                    new_rules.push(rule.take());
                }
            }
        }
        *rules = new_rules;
    }
}

fn emit_id_comment<W: Write>(
    output: &mut W,
    id: &(&JsWord, &Rc<Id>),
    module_index: u32,
    modules: &Vec<Box<CssMapModule>>,
) -> Result<(), std::fmt::Error> {
    if id.1.module_index == module_index {
        write!(output, "/** {} {{@link {}}} */\n", id.1.kind, id.0)
    } else {
        if id.0 == &id.1.local_id[..] {
            write!(
                output,
                "/** extern {} {{@link {}}} from '{}' */\n",
                id.1.kind, id.0, modules[id.1.module_index as usize].id
            )
        } else {
            write!(
                output,
                "/** extern {} {} as {{@link {}}} from '{}' */\n",
                id.1.kind, id.1.local_id, id.0, modules[id.1.module_index as usize].id
            )
        }
    }
}

fn emit_ts<W: Write>(
    output: &mut W,
    classes: &[(&JsWord, &Rc<Id>)],
    vars: &[(&JsWord, &Rc<Id>)],
    keyframes: &[(&JsWord, &Rc<Id>)],
    module_index: u32,
    class_maps: &[ClassMap],
    modules: &Vec<Box<CssMapModule>>,
) -> Result<(), std::fmt::Error> {
    write!(output, "/** class names */\n")?;
    write!(output, "export const enum c {{\n")?;
    for id in classes {
        write!(output, "  ")?;
        emit_id_comment(output, id, module_index, modules)?;
        write!(output, "  {} = \"{}\",\n", id.0, id.1.global_id)?;
    }
    write!(output, "}}\n")?;

    write!(output, "/** vars */\n")?;
    write!(output, "export const enum v {{\n")?;
    for id in vars {
        write!(output, "  ")?;
        emit_id_comment(output, id, module_index, modules)?;
        write!(output, "  {} = \"{}\",\n", id.0, id.1.global_id)?;
    }
    write!(output, "}}\n")?;

    write!(output, "/** keyframes */\n")?;
    write!(output, "export const enum k {{\n")?;
    for id in keyframes {
        write!(output, "  ")?;
        emit_id_comment(output, id, module_index, modules)?;
        write!(output, "  {} = \"{}\",\n", id.0, id.1.global_id)?;
    }
    write!(output, "}}\n")?;

    if !class_maps.is_empty() {
        output.write_char('\n')?;
        for cn in class_maps {
            cn.emit_ts(output)?;
        }
    }

    Ok(())
}

fn emit_js<W: Write>(
    output: &mut W,
    class_maps: &[ClassMap],
    class_map_output: ClassMapOutput,
) -> Result<(), std::fmt::Error> {
    if !class_maps.is_empty() {
        for cm in class_maps {
            cm.emit_js(output, class_map_output)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum CompilationError {}

#[derive(Debug)]
pub struct CompilationArtifact {
    pub css: String,
    pub js: String,
    pub ts: String,
}

fn is_valid_js_ident(ident: &str) -> bool {
    let mut iter = ident.chars();
    if let Some(next) = iter.next() {
        match next {
            'a'..='z' | 'A'..='Z' | '_' => {
                for c in iter {
                    match c {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {}
                        _ => return false,
                    }
                }
                true
            }
            _ => false,
        }
    } else {
        false
    }
}
