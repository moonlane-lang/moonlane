use crate::ast::Span;

// ── Error code enums, one per pipeline phase ──────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorCode {
    P0001, // Syntax error
    P0002, // Invalid integer literal
    P0003, // Invalid float literal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeErrorCode {
    T0001, // Type mismatch
    T0002, // Annotation required
    T0003, // Undefined name
    T0004, // Arity mismatch
    T0005, // Invalid operand types
    T0006, // Assignment to immutable binding
    T0007, // Invalid cast
    T0008, // Non-exhaustive match
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorCode {
    R0001, // No `main` function defined
    R0002, // `main` is not a valid entry point
    R0003, // Undefined variable at runtime
    R0004, // Index out of bounds
    R0005, // Tuple index out of bounds
    R0006, // Non-exhaustive match at runtime
    R0007, // Arithmetic error (division or remainder by zero)
    R0008, // Field not found
    R0009, // Method not found
    R0010, // Call on non-callable value
    R0011, // Invalid for-in iterator
    R0012, // Error propagation on non-Result value
    R0013, // Assertion failed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalErrorCode {
    I0001, // Internal interpreter error (interpreter bug — should never happen)
    I0002, // Not implemented (feature not yet supported in this version)
}

macro_rules! impl_display_via_debug {
    ($t:ty) => {
        impl std::fmt::Display for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{self:?}")
            }
        }
    };
}

impl_display_via_debug!(ParseErrorCode);
impl_display_via_debug!(TypeErrorCode);
impl_display_via_debug!(RuntimeErrorCode);
impl_display_via_debug!(InternalErrorCode);

// ── Error variants ────────────────────────────────────────────────────────────

/// All errors that can be produced at any stage of the pipeline.
#[derive(Debug)]
pub enum GustError {
    ParseError {
        code: ParseErrorCode,
        message: String,
        start: usize,
        end: usize,
        filename: String,
        line: u32,
        col: u32,
        /// Source line text, if available (from the pest grammar failure).
        source_line: Option<String>,
    },
    TypeError {
        code: TypeErrorCode,
        message: String,
        start: usize,
        end: usize,
        filename: String,
        line: u32,
        col: u32,
    },
    RuntimePanic {
        code: RuntimeErrorCode,
        message: String,
        start: usize,
        end: usize,
        filename: String,
        line: u32,
        col: u32,
    },
    /// A bug in the interpreter or an unimplemented feature — never caused by user input.
    Internal {
        code: InternalErrorCode,
        message: String,
    },
}

impl std::fmt::Display for GustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GustError::ParseError { code, message, filename, line, col, source_line: None, .. } =>
                write!(f, "[{code}] parse error in {filename}:{line}:{col}: {message}"),
            GustError::ParseError { code, message, filename, line, col, source_line: Some(src), .. } =>
                write!(f, "[{code}] parse error in {filename}:{line}:{col} (`{src}`): {message}"),
            GustError::TypeError { code, message, filename, line, col, .. } =>
                write!(f, "[{code}] type error in {filename}:{line}:{col}: {message}"),
            GustError::RuntimePanic { code, message, filename, line, col, .. } =>
                write!(f, "[{code}] runtime error in {filename}:{line}:{col}: {message}"),
            GustError::Internal { code, message } =>
                write!(f, "[{code}] internal error: {message}"),
        }
    }
}

impl std::error::Error for GustError {}

// ── Constructor helpers ───────────────────────────────────────────────────────

impl GustError {
    pub fn parse(code: ParseErrorCode, msg: impl Into<String>, span: &Span) -> Self {
        Self::ParseError {
            code,
            message: msg.into(),
            start: span.start,
            end: span.end,
            filename: span.filename.clone(),
            line: span.line,
            col: span.col,
            source_line: None,
        }
    }

    pub fn type_error(code: TypeErrorCode, msg: impl Into<String>, span: &Span) -> Self {
        Self::TypeError {
            code,
            message: msg.into(),
            start: span.start,
            end: span.end,
            filename: span.filename.clone(),
            line: span.line,
            col: span.col,
        }
    }

    pub fn panic(code: RuntimeErrorCode, msg: impl Into<String>, span: &Span) -> Self {
        Self::RuntimePanic {
            code,
            message: msg.into(),
            start: span.start,
            end: span.end,
            filename: span.filename.clone(),
            line: span.line,
            col: span.col,
        }
    }

    /// Interpreter bug — the typechecker should have prevented this state.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal { code: InternalErrorCode::I0001, message: msg.into() }
    }

    /// Feature not yet implemented in this version of the interpreter.
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::Internal { code: InternalErrorCode::I0002, message: msg.into() }
    }
}
