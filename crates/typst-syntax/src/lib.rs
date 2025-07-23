//! Parser and syntax tree for Typst.

pub mod ast;
pub mod package;

mod file;
mod highlight;
mod kind;
mod lexer;
mod lines;
mod node;
mod parser;
mod path;
mod reparser;
mod set;
mod source;
mod span;

use serde::Serialize;
use strum::IntoEnumIterator;
use wasm_bindgen::prelude::*;

pub use self::file::FileId;
pub use self::highlight::{Tag, highlight, highlight_html};
pub use self::kind::SyntaxKind;
pub use self::lexer::{
    is_id_continue, is_id_start, is_ident, is_newline, is_valid_label_literal_id,
    link_prefix, split_newlines,
};
pub use self::lines::Lines;
pub use self::node::{LinkedChildren, LinkedNode, Side, SyntaxError, SyntaxNode};
pub use self::parser::{parse, parse_code, parse_math};
pub use self::path::VirtualPath;
pub use self::source::Source;
pub use self::span::{Span, Spanned};

use self::lexer::Lexer;
use self::parser::{reparse_block, reparse_markup};

/// The syntax mode of a portion of Typst code.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SyntaxMode {
    /// Text and markup, as in the top level.
    Markup,
    /// Math atoms, operators, etc., as in equations.
    Math,
    /// Keywords, literals and operators, as after hashes.
    Code,
}

#[wasm_bindgen]
/// An incremental parser for usage in wasm.
pub struct TypstWasmParser {
    inner: Source,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildrenSplice {
    prefix: Vec<usize>,
    from: usize,
    to: usize,
    replacement: Vec<SyntaxNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateParent {
    /// The last element specifys the index of child that get updated.
    prefix: Vec<usize>,
    prev: usize,
    new: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum Edit {
    UpdateParent(UpdateParent),
    ChildrenSplice(ChildrenSplice),
}

#[derive(Debug, Clone, Default, Serialize)]
/// An edit to a parsed tree.
pub struct Edits {
    full_update: bool,
    edits: Vec<Edit>,
}

impl Edits {
    pub fn push(&mut self, edit: Edit) {
        self.edits.push(edit);
    }

    pub fn pop(&mut self) {
        self.edits.pop();
    }

    fn fail_incremental(&mut self) {
        self.edits.clear();
        self.full_update = true;
    }
}

#[wasm_bindgen]
impl TypstWasmParser {
    #[wasm_bindgen(constructor)]
    pub fn new(doc: String) -> Self {
        console_error_panic_hook::set_once();
        Self {
            inner: Source::new(FileId::new(None, VirtualPath::new("/main.typ")), doc),
        }
    }

    /// Edit the source
    ///
    /// Returns the corresponding edit for JS side.
    pub fn edit(
        &mut self,
        replace_from: usize,
        replace_to: usize,
        with: &str,
    ) -> JsValue {
        // Update the text and lines.
        let byte_from = self.inner.lines().utf16_to_byte(replace_from).unwrap();
        let byte_to = self.inner.lines().utf16_to_byte(replace_to).unwrap();
        let mut edits = Some(Edits::default());
        self.inner
            .edit(byte_from..byte_to, replace_to - replace_from, with, &mut edits);
        serde_wasm_bindgen::to_value(&edits).unwrap()
    }

    pub fn tree(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.inner.root()).unwrap()
    }

    pub fn get_node_types() -> JsValue {
        serde_wasm_bindgen::to_value(
            &SyntaxKind::iter()
                .map(|v| (Into::<&'static str>::into(v), v as u8))
                .collect::<Vec<(_, _)>>(),
        )
        .unwrap()
    }
}
