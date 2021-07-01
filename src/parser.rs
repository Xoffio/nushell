use std::str::Utf8Error;

use crate::{
    lex, lite_parse,
    parser_state::{Type, VarId},
    LiteBlock, LiteCommand, LiteStatement, ParseError, ParserWorkingSet, Span,
};

pub struct Signature {
    pub name: String,
    pub mandatory_positional: Vec<SyntaxShape>,
}

/// The syntactic shapes that values must match to be passed into a command. You can think of this as the type-checking that occurs when you call a function.
#[derive(Debug, Clone)]
pub enum SyntaxShape {
    /// A specific match to a word or symbol
    Word(Vec<u8>),
    /// Any syntactic form is allowed
    Any,
    /// Strings and string-like bare words are allowed
    String,
    /// A dotted path to navigate the table
    ColumnPath,
    /// A dotted path to navigate the table (including variable)
    FullColumnPath,
    /// Only a numeric (integer or decimal) value is allowed
    Number,
    /// A range is allowed (eg, `1..3`)
    Range,
    /// Only an integer value is allowed
    Int,
    /// A filepath is allowed
    FilePath,
    /// A glob pattern is allowed, eg `foo*`
    GlobPattern,
    /// A block is allowed, eg `{start this thing}`
    Block,
    /// A table is allowed, eg `[first second]`
    Table,
    /// A filesize value is allowed, eg `10kb`
    Filesize,
    /// A duration value is allowed, eg `19day`
    Duration,
    /// An operator
    Operator,
    /// A math expression which expands shorthand forms on the lefthand side, eg `foo > 1`
    /// The shorthand allows us to more easily reach columns inside of the row being passed in
    RowCondition,
    /// A general math expression, eg the `1 + 2` of `= 1 + 2`
    MathExpression,
}

#[derive(Debug)]
pub enum Expr {
    Int(i64),
    Var(VarId),
    Garbage,
}

#[derive(Debug)]
pub struct Expression {
    expr: Expr,
    ty: Type,
    span: Span,
}
impl Expression {
    pub fn garbage(span: Span) -> Expression {
        Expression {
            expr: Expr::Garbage,
            span,
            ty: Type::Unknown,
        }
    }
}

#[derive(Debug)]
pub enum Import {}

#[derive(Debug)]
pub struct Block {
    stmts: Vec<Statement>,
}

impl Default for Block {
    fn default() -> Self {
        Self::new()
    }
}

impl Block {
    pub fn new() -> Self {
        Self { stmts: vec![] }
    }
}

#[derive(Debug)]
pub struct VarDecl {
    var_id: VarId,
    expression: Expression,
}

#[derive(Debug)]
pub enum Statement {
    Pipeline(Pipeline),
    VarDecl(VarDecl),
    Import(Import),
    Expression(Expression),
    None,
}

#[derive(Debug)]
pub struct Pipeline {}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self {}
    }
}

fn garbage(span: Span) -> Expression {
    Expression::garbage(span)
}

fn is_identifier_byte(b: u8) -> bool {
    b != b'.' && b != b'[' && b != b'(' && b != b'{'
}

fn is_identifier(bytes: &[u8]) -> bool {
    bytes.iter().all(|x| is_identifier_byte(*x))
}

fn is_variable(bytes: &[u8]) -> bool {
    if bytes.len() > 1 && bytes[0] == b'$' {
        is_identifier(&bytes[1..])
    } else {
        is_identifier(bytes)
    }
}

fn span(spans: &[Span]) -> Span {
    let length = spans.len();

    if length == 0 {
        Span::unknown()
    } else if length == 1 || spans[0].file_id != spans[length - 1].file_id {
        spans[0]
    } else {
        Span {
            start: spans[0].start,
            end: spans[length - 1].end,
            file_id: spans[0].file_id,
        }
    }
}

impl ParserWorkingSet {
    pub fn parse_external_call(&mut self, spans: &[Span]) -> (Expression, Option<ParseError>) {
        // TODO: add external parsing
        (Expression::garbage(spans[0]), None)
    }

    pub fn parse_call(&mut self, spans: &[Span]) -> (Expression, Option<ParseError>) {
        // assume spans.len() > 0?
        let name = self.get_span_contents(spans[0]);

        if let Some(decl_id) = self.find_decl(name) {
            let sig = self.get_decl(decl_id).expect("internal error: bad DeclId");

            let mut positional_idx = 0;
            let mut arg_offset = 1;

            (Expression::garbage(spans[0]), None)
        } else {
            self.parse_external_call(spans)
        }
    }

    pub fn parse_int(&mut self, token: &str, span: Span) -> (Expression, Option<ParseError>) {
        if let Some(token) = token.strip_prefix("0x") {
            if let Ok(v) = i64::from_str_radix(token, 16) {
                (
                    Expression {
                        expr: Expr::Int(v),
                        ty: Type::Int,
                        span,
                    },
                    None,
                )
            } else {
                (
                    garbage(span),
                    Some(ParseError::Mismatch("int".into(), span)),
                )
            }
        } else if let Some(token) = token.strip_prefix("0b") {
            if let Ok(v) = i64::from_str_radix(token, 2) {
                (
                    Expression {
                        expr: Expr::Int(v),
                        ty: Type::Int,
                        span,
                    },
                    None,
                )
            } else {
                (
                    garbage(span),
                    Some(ParseError::Mismatch("int".into(), span)),
                )
            }
        } else if let Some(token) = token.strip_prefix("0o") {
            if let Ok(v) = i64::from_str_radix(token, 8) {
                (
                    Expression {
                        expr: Expr::Int(v),
                        ty: Type::Int,
                        span,
                    },
                    None,
                )
            } else {
                (
                    garbage(span),
                    Some(ParseError::Mismatch("int".into(), span)),
                )
            }
        } else if let Ok(x) = token.parse::<i64>() {
            (
                Expression {
                    expr: Expr::Int(x),
                    ty: Type::Int,
                    span,
                },
                None,
            )
        } else {
            (
                garbage(span),
                Some(ParseError::Mismatch("int".into(), span)),
            )
        }
    }

    pub fn parse_number(&mut self, token: &str, span: Span) -> (Expression, Option<ParseError>) {
        if let (x, None) = self.parse_int(token, span) {
            (x, None)
        } else {
            (
                garbage(span),
                Some(ParseError::Mismatch("number".into(), span)),
            )
        }
    }

    pub fn parse_arg(
        &mut self,
        span: Span,
        shape: SyntaxShape,
    ) -> (Expression, Option<ParseError>) {
        let bytes = self.get_span_contents(span);
        if !bytes.is_empty() && bytes[0] == b'$' {
            if let Some(var_id) = self.find_variable(bytes) {
                let ty = *self
                    .get_variable(var_id)
                    .expect("internal error: invalid VarId");
                return (
                    Expression {
                        expr: Expr::Var(var_id),
                        ty,
                        span,
                    },
                    None,
                );
            } else {
                return (garbage(span), Some(ParseError::VariableNotFound(span)));
            }
        }

        match shape {
            SyntaxShape::Number => {
                if let Ok(token) = String::from_utf8(bytes.into()) {
                    self.parse_number(&token, span)
                } else {
                    (
                        garbage(span),
                        Some(ParseError::Mismatch("number".into(), span)),
                    )
                }
            }
            _ => (
                garbage(span),
                Some(ParseError::Mismatch("number".into(), span)),
            ),
        }
    }

    pub fn parse_math_expression(&mut self, spans: &[Span]) -> (Expression, Option<ParseError>) {
        self.parse_arg(spans[0], SyntaxShape::Number)
    }

    pub fn parse_expression(&mut self, spans: &[Span]) -> (Expression, Option<ParseError>) {
        self.parse_math_expression(spans)
    }

    pub fn parse_variable(&mut self, span: Span) -> (Option<VarId>, Option<ParseError>) {
        let bytes = self.get_span_contents(span);

        if is_variable(bytes) {
            if let Some(var_id) = self.find_variable(bytes) {
                (Some(var_id), None)
            } else {
                (None, None)
            }
        } else {
            (None, Some(ParseError::Mismatch("variable".into(), span)))
        }
    }

    pub fn parse_keyword(&self, span: Span, keyword: &[u8]) -> Option<ParseError> {
        if self.get_span_contents(span) == keyword {
            None
        } else {
            Some(ParseError::Mismatch(
                String::from_utf8_lossy(keyword).to_string(),
                span,
            ))
        }
    }

    pub fn parse_let(&mut self, spans: &[Span]) -> (Statement, Option<ParseError>) {
        let mut error = None;
        if spans.len() >= 4 && self.parse_keyword(spans[0], b"let").is_none() {
            let (_, err) = self.parse_variable(spans[1]);
            error = error.or(err);

            let err = self.parse_keyword(spans[2], b"=");
            error = error.or(err);

            let (expression, err) = self.parse_expression(&spans[3..]);
            error = error.or(err);

            let var_name: Vec<_> = self.get_span_contents(spans[1]).into();
            let var_id = self.add_variable(var_name, expression.ty);

            (Statement::VarDecl(VarDecl { var_id, expression }), error)
        } else {
            let span = span(spans);
            (
                Statement::Expression(garbage(span)),
                Some(ParseError::Mismatch("let".into(), span)),
            )
        }
    }

    pub fn parse_statement(&mut self, spans: &[Span]) -> (Statement, Option<ParseError>) {
        if let (stmt, None) = self.parse_let(spans) {
            (stmt, None)
        } else if let (expr, None) = self.parse_expression(spans) {
            (Statement::Expression(expr), None)
        } else {
            let span = span(spans);
            (
                Statement::Expression(garbage(span)),
                Some(ParseError::Mismatch("statement".into(), span)),
            )
        }
    }

    pub fn parse_block(&mut self, lite_block: &LiteBlock) -> (Block, Option<ParseError>) {
        let mut error = None;
        self.enter_scope();

        let mut block = Block::new();

        for pipeline in &lite_block.block {
            let (stmt, err) = self.parse_statement(&pipeline.commands[0].parts);
            error = error.or(err);

            block.stmts.push(stmt);
        }

        self.exit_scope();

        (block, error)
    }

    pub fn parse_file(&mut self, fname: &str, contents: &[u8]) -> (Block, Option<ParseError>) {
        let mut error = None;

        let file_id = self.add_file(fname.into(), contents.into());

        let (output, err) = lex(contents, file_id, 0, crate::LexMode::Normal);
        error = error.or(err);

        let (output, err) = lite_parse(&output);
        error = error.or(err);

        let (output, err) = self.parse_block(&output);
        error = error.or(err);

        (output, error)
    }

    pub fn parse_source(&mut self, source: &[u8]) -> (Block, Option<ParseError>) {
        let mut error = None;

        let file_id = self.add_file("source".into(), source.into());

        let (output, err) = lex(source, file_id, 0, crate::LexMode::Normal);
        error = error.or(err);

        let (output, err) = lite_parse(&output);
        error = error.or(err);

        let (output, err) = self.parse_block(&output);
        error = error.or(err);

        (output, error)
    }
}
