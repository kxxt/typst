//! Parser and syntax tree for Typst.

pub mod ast;
pub mod package;

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

pub use self::highlight::{Tag, highlight, highlight_html};
pub use self::kind::SyntaxKind;
pub use self::lexer::{
    is_id_continue, is_id_start, is_ident, is_newline, is_valid_label_literal_id,
    link_prefix, split_newlines,
};
pub use self::lines::Lines;
pub use self::node::{
    Diagnosis, LinkedChildren, LinkedNode, Side, SyntaxDiagnostic, SyntaxNode,
};
pub use self::parser::{parse, parse_code, parse_math};
pub use self::path::{
    FileId, PathError, RealizeError, RootedPath, VirtualPath, VirtualRoot,
    VirtualizeError,
};
pub use self::source::Source;
pub use self::span::{
    DiagSpan, DiagSpanKind, RangeMapper, Span, SpanKind, SpanNumber, Spanned, SubRange,
};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use wasm_bindgen::prelude::*;

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

#[derive(Debug, Deserialize)]
struct TextEdit {
    from: usize,
    to: usize,
    insert: String,
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
            inner: Source::new(
                RootedPath::new(
                    VirtualRoot::Project,
                    VirtualPath::new("/main.typ").unwrap(),
                )
                .intern(),
                doc,
            ),
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
        self.inner.edit_with_edits(
            byte_from..byte_to,
            replace_to - replace_from,
            with,
            &mut edits,
        );
        serde_wasm_bindgen::to_value(&edits).unwrap()
    }

    /// Apply a group of edits whose ranges are all relative to the source
    /// before any of the edits were applied.
    pub fn edit_many(&mut self, edits: JsValue) {
        let edits: Vec<TextEdit> = serde_wasm_bindgen::from_value(edits).unwrap();
        let mut offset = 0isize;

        for edit in edits {
            let from = edit.from.checked_add_signed(offset).unwrap();
            let to = edit.to.checked_add_signed(offset).unwrap();
            let byte_from = self.inner.lines().utf16_to_byte(from).unwrap();
            let byte_to = self.inner.lines().utf16_to_byte(to).unwrap();
            let replaced_length = to - from;
            let replacement_length = edit.insert.encode_utf16().count();
            self.inner.edit_with_edits(
                byte_from..byte_to,
                replaced_length,
                &edit.insert,
                &mut None,
            );
            offset += replacement_length as isize - replaced_length as isize;
        }
    }

    /// Return syntax highlights as `(from, to, tag)` UTF-16 triples.
    pub fn highlight(&self) -> Box<[u32]> {
        fn visit(node: &LinkedNode, offset: usize, output: &mut Vec<u32>) {
            if let Some(tag) = highlight(node) {
                output.push(offset as u32);
                output.push((offset + node.length()) as u32);
                output.push(tag as u32);
            }

            let mut child_offset = offset;
            for child in node.children() {
                visit(&child, child_offset, output);
                child_offset += child.length();
            }
        }

        let mut output = vec![];
        visit(&LinkedNode::new(self.inner.root()), 0, &mut output);
        output.into_boxed_slice()
    }

    /// Get the CSS names of syntax highlight tags in tag-index order.
    pub fn get_highlight_tags() -> JsValue {
        serde_wasm_bindgen::to_value(
            &Tag::LIST.iter().map(|tag| tag.css_class()).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    /// The current source length in UTF-16 code units.
    pub fn length(&self) -> usize {
        self.inner.lines().len_utf16()
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
