//! NPC builder expression subset.
//!
//! Java roles use `BuilderExpression` documents such as
//! `{"Compute": "MaxHealth * 0.5"}`. This module ports the expression language
//! subset the role assets actually use: numbers, strings, booleans, arrays,
//! unary `! + -`, binary `+ - * / % **`, comparisons, equality, `&&`, `||`,
//! parentheses, array literals and the `StdLib` functions `max`, `min`,
//! `isEmpty`, `isEmptyStringArray`, `isEmptyNumberArray`, `makeRange`,
//! `random`, `randomInRange` and the `PI`/`true`/`false` constants.
//!
//! Types are strict like Java: arithmetic and comparisons are numeric,
//! equality accepts two numbers or two booleans, and logical operators accept
//! booleans. Unsupported syntax is a normal [`ExprError`].

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;
use thiserror::Error;

/// One typed expression value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Number (all Java expression numbers are doubles).
    Number(f64),
    /// Boolean.
    Bool(bool),
    /// String.
    String(String),
    /// Number array.
    NumberArray(Vec<f64>),
    /// String array.
    StringArray(Vec<String>),
    /// Boolean array.
    BoolArray(Vec<bool>),
    /// The untyped empty array (`[]`).
    EmptyArray,
}

impl Value {
    /// Java `ValueType` name.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Number(_) => "NUMBER",
            Self::Bool(_) => "BOOLEAN",
            Self::String(_) => "STRING",
            Self::NumberArray(_) => "NUMBER_ARRAY",
            Self::StringArray(_) => "STRING_ARRAY",
            Self::BoolArray(_) => "BOOLEAN_ARRAY",
            Self::EmptyArray => "EMPTY_ARRAY",
        }
    }

    /// Convert a literal JSON value to an expression value. Objects and
    /// heterogeneous arrays are not expression values.
    pub fn from_json(value: &JsonValue) -> Option<Self> {
        match value {
            JsonValue::Number(number) => number.as_f64().map(Self::Number),
            JsonValue::Bool(boolean) => Some(Self::Bool(*boolean)),
            JsonValue::String(string) => Some(Self::String(string.clone())),
            JsonValue::Array(values) => {
                if values.is_empty() {
                    return Some(Self::EmptyArray);
                }
                if let Some(numbers) = values
                    .iter()
                    .map(JsonValue::as_f64)
                    .collect::<Option<Vec<_>>>()
                {
                    return Some(Self::NumberArray(numbers));
                }
                if let Some(strings) = values
                    .iter()
                    .map(JsonValue::as_str)
                    .collect::<Option<Vec<_>>>()
                {
                    return Some(Self::StringArray(
                        strings.into_iter().map(ToOwned::to_owned).collect(),
                    ));
                }
                if let Some(booleans) = values
                    .iter()
                    .map(JsonValue::as_bool)
                    .collect::<Option<Vec<_>>>()
                {
                    return Some(Self::BoolArray(booleans));
                }
                None
            }
            _ => None,
        }
    }

    /// Convert back to a JSON literal.
    pub fn to_json(&self) -> JsonValue {
        match self {
            Self::Number(number) => serde_json::Number::from_f64(*number)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null),
            Self::Bool(boolean) => JsonValue::Bool(*boolean),
            Self::String(string) => JsonValue::String(string.clone()),
            Self::NumberArray(numbers) => JsonValue::Array(
                numbers
                    .iter()
                    .map(|number| {
                        serde_json::Number::from_f64(*number)
                            .map(JsonValue::Number)
                            .unwrap_or(JsonValue::Null)
                    })
                    .collect(),
            ),
            Self::StringArray(strings) => JsonValue::Array(
                strings
                    .iter()
                    .map(|string| JsonValue::String(string.clone()))
                    .collect(),
            ),
            Self::BoolArray(booleans) => JsonValue::Array(
                booleans
                    .iter()
                    .map(|boolean| JsonValue::Bool(*boolean))
                    .collect(),
            ),
            Self::EmptyArray => JsonValue::Array(Vec::new()),
        }
    }
}

/// Variables and the optional RNG used by `random`/`randomInRange`.
pub struct EvalContext<'a> {
    /// Expression variables (literal role parameters).
    pub vars: &'a BTreeMap<String, Value>,
    /// RNG for the two random builtins; `None` makes them an error.
    pub rng: Option<&'a mut dyn FnMut(f64, f64) -> f64>,
}

impl<'a> EvalContext<'a> {
    /// A context without an RNG.
    pub fn new(vars: &'a BTreeMap<String, Value>) -> Self {
        Self { vars, rng: None }
    }

    /// A context with an RNG.
    pub fn with_rng(
        vars: &'a BTreeMap<String, Value>,
        rng: &'a mut dyn FnMut(f64, f64) -> f64,
    ) -> Self {
        Self {
            vars,
            rng: Some(rng),
        }
    }
}

/// Evaluate one expression.
pub fn evaluate(expression: &str, context: &mut EvalContext<'_>) -> Result<Value, ExprError> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser {
        tokens,
        position: 0,
        context,
    };
    let value = parser.parse_expression()?;
    if parser.peek() != Token::End {
        return Err(ExprError::UnexpectedToken {
            position: parser.position,
        });
    }
    Ok(value)
}

/// Expression errors.
#[derive(Debug, Error, PartialEq)]
pub enum ExprError {
    /// Unexpected character.
    #[error("invalid character at {position}")]
    InvalidCharacter {
        /// Character offset.
        position: usize,
    },
    /// Unterminated string literal.
    #[error("unterminated string at {position}")]
    UnterminatedString {
        /// Character offset.
        position: usize,
    },
    /// Unexpected token.
    #[error("unexpected token at {position}")]
    UnexpectedToken {
        /// Token offset.
        position: usize,
    },
    /// Unknown variable.
    #[error("unknown variable `{0}`")]
    UnknownVariable(String),
    /// Type mismatch.
    #[error("type mismatch for `{operator}`")]
    TypeMismatch {
        /// Operator/function name.
        operator: String,
    },
    /// Unknown function.
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
    /// Wrong argument count.
    #[error("`{function}` expects {expected} argument(s), got {actual}")]
    Arity {
        /// Function name.
        function: String,
        /// Expected count.
        expected: usize,
        /// Actual count.
        actual: usize,
    },
    /// A random builtin was used without an RNG.
    #[error("`{0}` needs an RNG")]
    MissingRng(String),
    /// Division by zero.
    #[error("division by zero")]
    DivisionByZero,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Identifier(String),
    True,
    False,
    Pi,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    Bang,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Comma,
    End,
}

fn tokenize(expression: &str) -> Result<Vec<Token>, ExprError> {
    let bytes = expression.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'0'..=b'9' | b'.' => {
                let start = index;
                let mut has_dot = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'0'..=b'9' => index += 1,
                        b'.' if !has_dot => {
                            has_dot = true;
                            index += 1;
                        }
                        _ => break,
                    }
                }
                let text = &expression[start..index];
                let number = text
                    .parse::<f64>()
                    .map_err(|_| ExprError::InvalidCharacter { position: start })?;
                tokens.push(Token::Number(number));
            }
            b'"' => {
                let start = index;
                index += 1;
                let mut string = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'"' => {
                            index += 1;
                            closed = true;
                            break;
                        }
                        b'\\' if index + 1 < bytes.len() => {
                            index += 1;
                            string.push(match bytes[index] {
                                b'n' => '\n',
                                b't' => '\t',
                                other => char::from(other),
                            });
                            index += 1;
                        }
                        other => {
                            string.push(char::from(other));
                            index += 1;
                        }
                    }
                }
                if !closed {
                    return Err(ExprError::UnterminatedString { position: start });
                }
                tokens.push(Token::String(string));
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                let identifier = &expression[start..index];
                tokens.push(match identifier {
                    "true" => Token::True,
                    "false" => Token::False,
                    "PI" => Token::Pi,
                    other => Token::Identifier(other.to_string()),
                });
            }
            b'+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            b'-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            b'*' => {
                if bytes.get(index + 1) == Some(&b'*') {
                    tokens.push(Token::StarStar);
                    index += 2;
                } else {
                    tokens.push(Token::Star);
                    index += 1;
                }
            }
            b'/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            b'%' => {
                tokens.push(Token::Percent);
                index += 1;
            }
            b'!' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Token::BangEqual);
                    index += 2;
                } else {
                    tokens.push(Token::Bang);
                    index += 1;
                }
            }
            b'=' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Token::EqualEqual);
                    index += 2;
                } else {
                    return Err(ExprError::InvalidCharacter { position: index });
                }
            }
            b'<' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Token::LessEqual);
                    index += 2;
                } else {
                    tokens.push(Token::Less);
                    index += 1;
                }
            }
            b'>' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Token::GreaterEqual);
                    index += 2;
                } else {
                    tokens.push(Token::Greater);
                    index += 1;
                }
            }
            b'&' => {
                if bytes.get(index + 1) == Some(&b'&') {
                    tokens.push(Token::AndAnd);
                    index += 2;
                } else {
                    return Err(ExprError::InvalidCharacter { position: index });
                }
            }
            b'|' => {
                if bytes.get(index + 1) == Some(&b'|') {
                    tokens.push(Token::OrOr);
                    index += 2;
                } else {
                    return Err(ExprError::InvalidCharacter { position: index });
                }
            }
            b'(' => {
                tokens.push(Token::OpenParen);
                index += 1;
            }
            b')' => {
                tokens.push(Token::CloseParen);
                index += 1;
            }
            b'[' => {
                tokens.push(Token::OpenBracket);
                index += 1;
            }
            b']' => {
                tokens.push(Token::CloseBracket);
                index += 1;
            }
            b',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            _ => return Err(ExprError::InvalidCharacter { position: index }),
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

struct Parser<'a, 'b> {
    tokens: Vec<Token>,
    position: usize,
    context: &'a mut EvalContext<'b>,
}

impl Parser<'_, '_> {
    fn peek(&self) -> Token {
        self.tokens
            .get(self.position)
            .cloned()
            .unwrap_or(Token::End)
    }

    fn next(&mut self) -> Token {
        let token = self.peek();
        self.position += 1;
        token
    }

    fn parse_expression(&mut self) -> Result<Value, ExprError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_and()?;
        while self.peek() == Token::OrOr {
            self.next();
            let right = self.parse_and()?;
            left = match (left, right) {
                (Value::Bool(left), Value::Bool(right)) => Value::Bool(left || right),
                _ => {
                    return Err(ExprError::TypeMismatch {
                        operator: "||".to_string(),
                    })
                }
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_equality()?;
        while self.peek() == Token::AndAnd {
            self.next();
            let right = self.parse_equality()?;
            left = match (left, right) {
                (Value::Bool(left), Value::Bool(right)) => Value::Bool(left && right),
                _ => {
                    return Err(ExprError::TypeMismatch {
                        operator: "&&".to_string(),
                    })
                }
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_comparison()?;
        loop {
            let operator = match self.peek() {
                Token::EqualEqual => "==",
                Token::BangEqual => "!=",
                _ => break,
            };
            self.next();
            let right = self.parse_comparison()?;
            let equal = match (&left, &right) {
                (Value::Number(left), Value::Number(right)) => left == right,
                (Value::Bool(left), Value::Bool(right)) => left == right,
                _ => {
                    return Err(ExprError::TypeMismatch {
                        operator: operator.to_string(),
                    })
                }
            };
            left = Value::Bool(if operator == "==" { equal } else { !equal });
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_additive()?;
        loop {
            let operator = match self.peek() {
                Token::Less => "<",
                Token::LessEqual => "<=",
                Token::Greater => ">",
                Token::GreaterEqual => ">=",
                _ => break,
            };
            self.next();
            let right = self.parse_additive()?;
            let (Value::Number(lhs), Value::Number(rhs)) = (left, right) else {
                return Err(ExprError::TypeMismatch {
                    operator: operator.to_string(),
                });
            };
            let result = match operator {
                "<" => lhs < rhs,
                "<=" => lhs <= rhs,
                ">" => lhs > rhs,
                _ => lhs >= rhs,
            };
            left = Value::Bool(result);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let operator = match self.peek() {
                Token::Plus => "+",
                Token::Minus => "-",
                _ => break,
            };
            self.next();
            let right = self.parse_multiplicative()?;
            let (Value::Number(lhs), Value::Number(rhs)) = (left, right) else {
                return Err(ExprError::TypeMismatch {
                    operator: operator.to_string(),
                });
            };
            let result = if operator == "+" {
                lhs + rhs
            } else {
                lhs - rhs
            };
            left = Value::Number(result);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Value, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = match self.peek() {
                Token::Star => "*",
                Token::Slash => "/",
                Token::Percent => "%",
                _ => break,
            };
            self.next();
            let right = self.parse_unary()?;
            let (Value::Number(lhs), Value::Number(rhs)) = (left, right) else {
                return Err(ExprError::TypeMismatch {
                    operator: operator.to_string(),
                });
            };
            let result = match operator {
                "*" => lhs * rhs,
                "/" => {
                    if rhs == 0.0 {
                        return Err(ExprError::DivisionByZero);
                    }
                    lhs / rhs
                }
                _ => lhs % rhs,
            };
            left = Value::Number(result);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Value, ExprError> {
        match self.next() {
            Token::Minus => match self.parse_unary()? {
                Value::Number(number) => Ok(Value::Number(-number)),
                _ => Err(ExprError::TypeMismatch {
                    operator: "unary -".to_string(),
                }),
            },
            Token::Plus => match self.parse_unary()? {
                Value::Number(number) => Ok(Value::Number(number)),
                _ => Err(ExprError::TypeMismatch {
                    operator: "unary +".to_string(),
                }),
            },
            Token::Bang => match self.parse_unary()? {
                Value::Bool(boolean) => Ok(Value::Bool(!boolean)),
                _ => Err(ExprError::TypeMismatch {
                    operator: "!".to_string(),
                }),
            },
            Token::StarStar => Err(ExprError::TypeMismatch {
                operator: "**".to_string(),
            }),
            token => self.parse_primary(token),
        }
    }

    fn parse_primary(&mut self, token: Token) -> Result<Value, ExprError> {
        match token {
            Token::Number(number) => Ok(Value::Number(number)),
            Token::String(string) => Ok(Value::String(string)),
            Token::True => Ok(Value::Bool(true)),
            Token::False => Ok(Value::Bool(false)),
            Token::Pi => Ok(Value::Number(f64::from(hytale_math::trig_math_util::PI))),
            Token::Identifier(name) => {
                if self.peek() == Token::OpenParen {
                    self.next();
                    let arguments = self.parse_arguments()?;
                    self.call_function(&name, arguments)
                } else if let Some(value) = self.context.vars.get(&name) {
                    Ok(value.clone())
                } else {
                    Err(ExprError::UnknownVariable(name))
                }
            }
            Token::OpenParen => {
                let value = self.parse_expression()?;
                if self.next() != Token::CloseParen {
                    return Err(ExprError::UnexpectedToken {
                        position: self.position,
                    });
                }
                Ok(value)
            }
            Token::OpenBracket => {
                let values = self.parse_arguments()?;
                pack_array(values)
            }
            _ => Err(ExprError::UnexpectedToken {
                position: self.position,
            }),
        }
    }

    fn parse_arguments(&mut self) -> Result<Vec<Value>, ExprError> {
        let mut values = Vec::new();
        if self.peek() == Token::CloseParen || self.peek() == Token::CloseBracket {
            let close = self.next();
            return if close == Token::CloseParen || close == Token::CloseBracket {
                Ok(values)
            } else {
                Err(ExprError::UnexpectedToken {
                    position: self.position,
                })
            };
        }
        loop {
            values.push(self.parse_expression()?);
            match self.next() {
                Token::Comma => continue,
                Token::CloseParen | Token::CloseBracket => break,
                _ => {
                    return Err(ExprError::UnexpectedToken {
                        position: self.position,
                    })
                }
            }
        }
        Ok(values)
    }

    fn call_function(&mut self, name: &str, mut arguments: Vec<Value>) -> Result<Value, ExprError> {
        match name {
            "max" | "min" => {
                self.expect_arity(name, arguments.len(), 2)?;
                let right = arguments.pop().expect("arity checked");
                let left = arguments.pop().expect("arity checked");
                let (Value::Number(left), Value::Number(right)) = (left, right) else {
                    return Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    });
                };
                Ok(Value::Number(if name == "max" {
                    left.max(right)
                } else {
                    left.min(right)
                }))
            }
            "isEmpty" => {
                self.expect_arity(name, arguments.len(), 1)?;
                match arguments.pop().expect("arity checked") {
                    Value::String(string) => Ok(Value::Bool(string.is_empty())),
                    _ => Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    }),
                }
            }
            "isEmptyStringArray" => {
                self.expect_arity(name, arguments.len(), 1)?;
                match arguments.pop().expect("arity checked") {
                    Value::StringArray(strings) => Ok(Value::Bool(strings.is_empty())),
                    Value::EmptyArray => Ok(Value::Bool(true)),
                    _ => Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    }),
                }
            }
            "isEmptyNumberArray" => {
                self.expect_arity(name, arguments.len(), 1)?;
                match arguments.pop().expect("arity checked") {
                    Value::NumberArray(numbers) => Ok(Value::Bool(numbers.is_empty())),
                    Value::EmptyArray => Ok(Value::Bool(true)),
                    _ => Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    }),
                }
            }
            "makeRange" => {
                self.expect_arity(name, arguments.len(), 1)?;
                let Value::Number(number) = arguments.pop().expect("arity checked") else {
                    return Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    });
                };
                Ok(Value::NumberArray(vec![number, number]))
            }
            "random" => {
                self.expect_arity(name, arguments.len(), 0)?;
                let Some(rng) = self.context.rng.as_deref_mut() else {
                    return Err(ExprError::MissingRng("random".to_string()));
                };
                Ok(Value::Number(rng(0.0, 1.0)))
            }
            "randomInRange" => {
                self.expect_arity(name, arguments.len(), 2)?;
                let high = arguments.pop().expect("arity checked");
                let low = arguments.pop().expect("arity checked");
                let (Value::Number(low), Value::Number(high)) = (low, high) else {
                    return Err(ExprError::TypeMismatch {
                        operator: name.to_string(),
                    });
                };
                let Some(rng) = self.context.rng.as_deref_mut() else {
                    return Err(ExprError::MissingRng("randomInRange".to_string()));
                };
                Ok(Value::Number(rng(low, high)))
            }
            _ => Err(ExprError::UnknownFunction(name.to_string())),
        }
    }

    fn expect_arity(&self, name: &str, actual: usize, expected: usize) -> Result<(), ExprError> {
        if actual != expected {
            return Err(ExprError::Arity {
                function: name.to_string(),
                expected,
                actual,
            });
        }
        Ok(())
    }
}

fn pack_array(values: Vec<Value>) -> Result<Value, ExprError> {
    if values.is_empty() {
        return Ok(Value::EmptyArray);
    }
    if let Some(numbers) = values
        .iter()
        .map(|value| match value {
            Value::Number(number) => Some(*number),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        return Ok(Value::NumberArray(numbers));
    }
    if let Some(strings) = values
        .iter()
        .map(|value| match value {
            Value::String(string) => Some(string.clone()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        return Ok(Value::StringArray(strings));
    }
    if let Some(booleans) = values
        .iter()
        .map(|value| match value {
            Value::Bool(boolean) => Some(*boolean),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
    {
        return Ok(Value::BoolArray(booleans));
    }
    Err(ExprError::TypeMismatch {
        operator: "array".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("a".to_string(), Value::Number(2.0)),
            ("b".to_string(), Value::Number(9.0)),
            ("flag".to_string(), Value::Bool(true)),
            ("name".to_string(), Value::String("x".to_string())),
            ("values".to_string(), Value::NumberArray(vec![1.0, 2.0])),
        ])
    }

    #[test]
    fn arithmetic_comparisons_and_logic_follow_java_types() {
        let vars = scope();
        let mut context = EvalContext::new(&vars);
        assert_eq!(evaluate("a + b * 2", &mut context), Ok(Value::Number(20.0)));
        assert_eq!(
            evaluate("a > 1 && b < 10", &mut context),
            Ok(Value::Bool(true))
        );
        assert_eq!(evaluate("!flag", &mut context), Ok(Value::Bool(false)));
        assert_eq!(evaluate("max(a, b)", &mut context), Ok(Value::Number(9.0)));
        assert_eq!(
            evaluate("[a + 1, b * 2]", &mut context),
            Ok(Value::NumberArray(vec![3.0, 18.0]))
        );
        assert_eq!(
            evaluate("makeRange(a)", &mut context),
            Ok(Value::NumberArray(vec![2.0, 2.0]))
        );
        assert_eq!(
            evaluate("isEmptyStringArray(values)", &mut context),
            Err(ExprError::TypeMismatch {
                operator: "isEmptyStringArray".to_string()
            })
        );
        assert_eq!(evaluate("a == 2", &mut context), Ok(Value::Bool(true)));
    }

    #[test]
    fn rng_builtins_need_an_explicit_source() {
        let vars = scope();
        let mut context = EvalContext::new(&vars);
        assert_eq!(
            evaluate("randomInRange(0.2, 0.4)", &mut context),
            Err(ExprError::MissingRng("randomInRange".to_string()))
        );

        let mut rng = |low: f64, high: f64| low + (high - low) * 0.5;
        let mut context = EvalContext::with_rng(&vars, &mut rng);
        let value = evaluate("randomInRange(0.2, 0.4)", &mut context).expect("random value");
        let Value::Number(number) = value else {
            panic!("expected a number");
        };
        assert!((number - 0.3).abs() < 1e-12);
    }
}
