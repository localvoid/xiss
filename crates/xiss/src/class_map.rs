use std::fmt::{self, Write};

use clap::ValueEnum;
use swc_atoms::JsWord;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ClassMapOutput {
    Inline,
    Table,
}

impl fmt::Display for ClassMapOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_possible_value().unwrap().get_name().fmt(f)
    }
}

#[derive(Debug, Clone)]
pub struct ClassMapState {
    pub name: JsWord,
    pub classes: String,
}

impl ClassMapState {
    pub fn new(name: JsWord, classes: String) -> Self {
        Self { name, classes }
    }
}

#[derive(Debug, Clone)]
pub struct ClassMap {
    // @classmap NAME { ... }
    pub name: JsWord,
    // @static static-class-a static-class-b;
    pub static_classes: String,
    // disabled: class-disabled-1 class-disabled-2;
    pub states: Vec<ClassMapState>,
    // @exclude active and disabled;
    // => maps to state indexes
    pub exclude_constraints: Vec<usize>,
}

impl ClassMap {
    pub fn new(
        name: JsWord,
        static_classes: String,
        states: Vec<ClassMapState>,
        exclude_constraints: Vec<usize>,
    ) -> Self {
        Self {
            name,
            static_classes,
            states,
            exclude_constraints,
        }
    }

    pub fn emit_js<W: Write>(
        &self,
        output: &mut W,
        kind: ClassMapOutput,
    ) -> Result<(), std::fmt::Error> {
        if let ClassMapOutput::Table = kind {
            // table
            let mut table = self.create_empty_table();
            self.populate_table(&mut table, 0, &self.static_classes, 0);

            write!(output, "const __CLASS_MAP_{} = [\n", self.name)?;
            for entry in table {
                write!(output, "  \"{}\",\n", entry)?;
            }
            write!(output, "];\n\n")?;
            write!(output, "/** classmap {{@link {}}} */\n", self.name)?;
            write!(output, "export function {}(", self.name)?;
            let mut iter = self.states.iter();
            if let Some(s) = iter.next() {
                write!(output, "{}", s.name)?;
                for s in iter {
                    write!(output, ", {}", s.name)?;
                }
            }
            write!(output, ") {{\n return __CLASS_MAP_{}[", self.name)?;
            let mut iter = self.states.iter().enumerate();
            if let Some((_, s)) = iter.next() {
                write!(output, "({} ? 1 : 0)", s.name)?;

                for (i, s) in iter {
                    write!(output, " | ({} ? {} : 0)", s.name, 1 << i)?;
                }
            }
            write!(output, "];\n}}\n")?;
        } else {
            // inline
            write!(output, "/** classmap {{@link {}}} */\n", self.name)?;
            write!(output, "export function {}(", self.name)?;
            let mut iter = self.states.iter();
            if let Some(s) = iter.next() {
                write!(output, "{}", s.name)?;
                for s in iter {
                    write!(output, ", {}", s.name)?;
                }
            }
            write!(output, ") {{\n return ")?;
            self.write_inline_cond_expr(output, 0, &self.static_classes, 0)?;
            write!(output, ";\n}}\n")?;
        }

        Ok(())
    }

    pub fn emit_ts<W: Write>(&self, output: &mut W) -> Result<(), std::fmt::Error> {
        write!(output, "/** classmap {{@link {}}} */\n", self.name)?;
        write!(output, "export function {}(", self.name)?;
        let mut iter = self.states.iter();
        if let Some(s) = iter.next() {
            write!(output, "{}: boolean", s.name)?;
            for s in iter {
                write!(output, ", {}: boolean", s.name)?;
            }
        }
        write!(output, "): string;\n")?;
        Ok(())
    }

    fn write_inline_cond_expr<W: Write>(
        &self,
        output: &mut W,
        mut i: usize,
        prev_state: &str,
        prev_state_mask: usize,
    ) -> Result<(), std::fmt::Error> {
        while i < self.states.len() {
            let s = &self.states[i];
            let next_state = join_strings(prev_state, &s.classes);
            let next_state_mask = prev_state_mask | (1 << i);
            i += 1;
            if self.is_constraints_satisfied(next_state_mask) {
                write!(output, "({} ? ", s.name)?;
                self.write_inline_cond_expr(output, i, &next_state, next_state_mask)?;
                write!(output, " : ")?;
                self.write_inline_cond_expr(output, i, prev_state, prev_state_mask)?;
                write!(output, ")")?;

                return Ok(());
            }
        }
        write!(output, "\"{}\"", prev_state)
    }

    fn create_empty_table(&self) -> Vec<String> {
        vec![String::new(); 2_usize.pow(self.states.len() as u32)]
    }

    fn populate_table(
        &self,
        result: &mut Vec<String>,
        i: usize,
        prev_state: &str,
        prev_state_mask: usize,
    ) {
        if !self.is_constraints_satisfied(prev_state_mask) {
            return;
        }

        if i == self.states.len() {
            result[prev_state_mask] = prev_state.into();
        } else {
            let next_i = i + 1;
            self.populate_table(result, next_i, prev_state, prev_state_mask);
            self.populate_table(
                result,
                next_i,
                &join_strings(prev_state, &self.states[i].classes),
                prev_state_mask | (1 << i),
            )
        }
    }

    /// Checks index bitmask in excluded constraints.
    fn is_constraints_satisfied(&self, index: usize) -> bool {
        for constraint in self.exclude_constraints.iter() {
            if (index & *constraint) == *constraint {
                return false;
            }
        }
        true
    }
}

/// Joins strings [a] and [b] with ' ' separator.
fn join_strings(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        let mut s = String::with_capacity(a.len() + b.len() + 1);
        s.push_str(a);
        s.push(' ');
        s.push_str(b);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_class_names_empty_a() {
        assert_eq!(join_strings("", "b"), "b");
    }

    #[test]
    fn concat_class_names_empty_a_b() {
        assert_eq!(join_strings("", ""), "");
    }

    #[test]
    fn concat_class_names_empty_b() {
        assert_eq!(join_strings("a", ""), "a ");
    }

    #[test]
    fn concat_class_names_a_b() {
        assert_eq!(join_strings("a", "b"), "a b");
    }

    #[test]
    fn is_constraints_satisfied_empty() {
        let cm = ClassMap::new("".into(), "".into(), vec![], vec![]);
        assert!(cm.is_constraints_satisfied(0b1))
    }

    #[test]
    fn is_constraints_satisfied_exclude_one_rule() {
        let cm = ClassMap::new("".into(), "".into(), vec![], vec![0b01]);
        assert!(cm.is_constraints_satisfied(0b10));

        assert!(!cm.is_constraints_satisfied(0b01));
    }

    #[test]
    fn is_constraints_satisfied_exclude_two_rules() {
        let cm = ClassMap::new("".into(), "".into(), vec![], vec![0b101, 0b110]);
        assert!(cm.is_constraints_satisfied(0b100));
        assert!(cm.is_constraints_satisfied(0b010));
        assert!(cm.is_constraints_satisfied(0b001));

        assert!(!cm.is_constraints_satisfied(0b101));
        assert!(!cm.is_constraints_satisfied(0b110));
    }

    #[test]
    fn write_class_map_inline_cond_expr_1() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![ClassMapState::new("a".into(), "A".into())],
            vec![],
        );

        let mut result = String::new();
        cm.write_inline_cond_expr(&mut result, 0, "", 0).unwrap();
        assert_eq!(result, "(a ? \"A\" : \"\")");
    }

    #[test]
    fn write_class_map_inline_cond_expr_2() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
            ],
            vec![],
        );

        let mut result = String::new();
        cm.write_inline_cond_expr(&mut result, 0, "", 0).unwrap();
        assert_eq!(result, "(a ? (b ? \"A B\" : \"A\") : (b ? \"B\" : \"\"))");
    }

    #[test]
    fn write_class_map_inline_cond_expr_exclude_0b11() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
            ],
            vec![0b11],
        );

        let mut result = String::new();
        cm.write_inline_cond_expr(&mut result, 0, "", 0).unwrap();
        assert_eq!(result, "(a ? \"A\" : (b ? \"B\" : \"\"))");
    }

    #[test]
    fn generate_class_map_table_0() {
        let cm = ClassMap::new("".into(), "".into(), vec![], vec![]);

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn generate_class_map_table_1() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![ClassMapState::new("a".into(), "A".into())],
            vec![],
        );

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(result, vec!["", "A"]);
    }

    #[test]
    fn generate_class_map_table_2() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
            ],
            vec![],
        );

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(result, vec!["", "A", "B", "A B"]);
    }

    #[test]
    fn generate_class_map_table_3() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
                ClassMapState::new("c".into(), "C".into()),
            ],
            vec![],
        );

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(
            result,
            vec!["", "A", "B", "A B", "C", "A C", "B C", "A B C"]
        );
    }

    #[test]
    fn generate_class_map_table_3_exclude_0b011() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
                ClassMapState::new("c".into(), "C".into()),
            ],
            vec![0b011],
        );

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(result, vec!["", "A", "B", "", "C", "A C", "B C", ""]);
    }

    #[test]
    fn generate_class_map_table_3_exclude_0b011_and_0b101() {
        let cm = ClassMap::new(
            "".into(),
            "".into(),
            vec![
                ClassMapState::new("a".into(), "A".into()),
                ClassMapState::new("b".into(), "B".into()),
                ClassMapState::new("c".into(), "C".into()),
            ],
            vec![0b011, 0b101],
        );

        let mut result = cm.create_empty_table();
        cm.populate_table(&mut result, 0, "", 0);
        assert_eq!(result, vec!["", "A", "B", "", "C", "", "B C", ""]);
    }
}
