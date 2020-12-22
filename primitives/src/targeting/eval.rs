use crate::BigNum;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::{value::Value as SerdeValue, Number};
use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt,
    ops::{Add, Div, Mul, Rem, Sub},
    str::FromStr,
};

pub use rules::Rules;

use super::{
    input::{channel::Getter as ChannelGetter, Get},
    Input, Output,
};

#[cfg(test)]
#[path = "eval_test.rs"]
mod test;

#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    TypeError,
    UnknownVariable,
}
pub const DAI_ADDR: &str = "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359";
pub const USDT_ADDR: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";
pub const USDC_ADDR: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

lazy_static! {
    pub static ref DEPOSIT_ASSETS_MAP: HashMap<String, BigNum> = {
        let mut assets = HashMap::new();
        assets.insert(DAI_ADDR.into(), BigNum::from(10u64.pow(18)));
        assets.insert(USDT_ADDR.into(), BigNum::from(10u64.pow(6)));
        assets.insert(USDC_ADDR.into(), BigNum::from(10u64.pow(18)));

        assets
    };
}

trait Eval {
    fn eval(self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error>;
}

impl Eval for Value {
    fn eval(self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error> {
        eval(input, output, &Rule::Value(self))
    }
}

impl Eval for Function {
    fn eval(self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error> {
        eval(input, output, &Rule::Function(self))
    }
}

impl Eval for &Rule {
    fn eval(self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error> {
        eval(input, output, self)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TypeError => write!(f, "TypeError: Wrong type"),
            Error::UnknownVariable => write!(f, "UnknownVariable: Unknown variable passed"),
        }
    }
}

impl std::error::Error for Error {}

mod rules {
    use serde::{
        de::{SeqAccess, Visitor},
        Deserialize, Deserializer, Serialize,
    };
    use std::{
        fmt,
        ops::{Deref, DerefMut},
    };

    use super::Rule;

    #[derive(Serialize, Debug, Default, Clone, Eq, PartialEq)]
    #[serde(transparent)]
    /// The Rules is just a `Vec<Rule>` with one difference:
    /// When Deserializing it will skip invalid `Rule` instead of returning an error
    pub struct Rules(pub Vec<Rule>);

    impl Rules {
        pub fn new() -> Self {
            Self(vec![])
        }
    }

    impl Deref for Rules {
        type Target = Vec<Rule>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for Rules {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<'de> Deserialize<'de> for Rules {
        fn deserialize<D>(deserializer: D) -> Result<Rules, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_seq(RulesVisitor)
        }
    }

    struct RulesVisitor;

    impl<'de> Visitor<'de> for RulesVisitor {
        type Value = Rules;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a sequence of Rules")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));

            // Since we want to filter wrong Rules, instead of returning an error
            // we transpose the `Result<Option<T>, ..>` to `Option<Result<T, ..>>`
            while let Some(result) = seq.next_element().transpose() {
                // push only valid rules
                if let Ok(rule) = result {
                    vec.push(rule);
                }
            }

            Ok(Rules(vec))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Rule {
    Function(Function),
    Value(Value),
}

impl Rule {
    pub fn eval(&self, input: &Input, output: &mut Output) -> Result<Option<Value>, Error> {
        eval(input, output, self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged, try_from = "SerdeValue", /* into = "SerdeValue" */)]
pub enum Value {
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    BigNum(BigNum),
}

impl Value {
    pub fn new_string(string: &str) -> Self {
        Self::String(string.to_string())
    }

    pub fn new_number(number: impl Into<Number>) -> Self {
        Self::Number(number.into())
    }
}

impl TryFrom<SerdeValue> for Value {
    type Error = Error;

    fn try_from(serde_value: SerdeValue) -> Result<Self, Self::Error> {
        match serde_value {
            SerdeValue::Bool(bool) => Ok(Self::Bool(bool)),
            SerdeValue::Number(number) => Ok(Self::Number(number)),
            // It's impossible to have a BigNumber literal in the rules, since they're JSON based (conform to serde_json::value::Value)
            // However it is possible to obtain a BigNumber by invoking the Function::Bn
            SerdeValue::String(string) => Ok(Value::String(string)),
            SerdeValue::Array(serde_array) => {
                let array = serde_array
                    .into_iter()
                    .map(Value::try_from)
                    .collect::<Result<_, _>>()?;
                Ok(Self::Array(array))
            }
            SerdeValue::Object(_) | SerdeValue::Null => Err(Error::TypeError),
        }
    }
}

impl Into<SerdeValue> for Value {
    fn into(self) -> SerdeValue {
        match self {
            Value::Bool(bool) => SerdeValue::Bool(bool),
            Value::Number(number) => SerdeValue::Number(number),
            Value::String(string) => SerdeValue::String(string),
            Value::Array(array) => {
                SerdeValue::Array(array.into_iter().map(|value| value.into()).collect())
            }
            Value::BigNum(bignum) => SerdeValue::String(bignum.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
// TODO: https://github.com/AdExNetwork/adex-validator-stack-rust/issues/296
pub enum Function {
    /// Multiplies first two values and then divides product by third value
    MulDiv(Box<Rule>, Box<Rule>, Box<Rule>),
    Div(Box<Rule>, Box<Rule>),
    Mul(Box<Rule>, Box<Rule>),
    Mod(Box<Rule>, Box<Rule>),
    Add(Box<Rule>, Box<Rule>),
    Sub(Box<Rule>, Box<Rule>),
    Max(Box<Rule>, Box<Rule>),
    Min(Box<Rule>, Box<Rule>),
    If(Box<Rule>, Box<Rule>),
    IfNot(Box<Rule>, Box<Rule>),
    IfElse(Box<Rule>, Box<Rule>, Box<Rule>),
    And(Box<Rule>, Box<Rule>),
    Or(Box<Rule>, Box<Rule>),
    Xor(Box<Rule>, Box<Rule>),
    Not(Box<Rule>),
    /// Is the first value Lesser than second value
    Lt(Box<Rule>, Box<Rule>),
    /// Is the first value Lesser than or equal to the second value
    Lte(Box<Rule>, Box<Rule>),
    /// Is the first value Greater than second value
    Gt(Box<Rule>, Box<Rule>),
    /// Is the first value Greater than or equal to the second value
    Gte(Box<Rule>, Box<Rule>),
    /// Are values equal
    Eq(Box<Rule>, Box<Rule>),
    /// Are values NOT equal
    Neq(Box<Rule>, Box<Rule>),
    /// Is first value included in an array (second value)
    In(Box<Rule>, Box<Rule>),
    /// Is first value NOT included in an array (second value)
    Nin(Box<Rule>, Box<Rule>),
    /// Gets the element at a certain position (second value) of an array (first value)
    At(Box<Rule>, Box<Rule>),
    /// Note: this is inclusive of the start and end value
    /// 0 - start
    /// 1 - end
    /// 2 - value
    Between(Box<Rule>, Box<Rule>, Box<Rule>),
    Split(Box<Rule>, Box<Rule>),
    StartsWith(Box<Rule>, Box<Rule>),
    EndsWith(Box<Rule>, Box<Rule>),
    OnlyShowIf(Box<Rule>),
    GetPriceInUsd(Box<Rule>),
    Intersects(Box<Rule>, Box<Rule>),
    /// Evaluates rule
    Do(Box<Rule>),
    Get(String),
    /// Output variables can be set any number of times by different rules, except `show`
    /// if `show` is at any point set to `false`, we stop executing rules and don't show the ad.
    Set(String, Box<Rule>),
    /// Bn(Value) function.
    Bn(Value),
}

impl From<Function> for Rule {
    fn from(function: Function) -> Self {
        Self::Function(function)
    }
}

impl From<Value> for Rule {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl Function {
    pub fn new_muldiv(
        value: impl Into<Rule>,
        multiplier: impl Into<Rule>,
        divisor: impl Into<Rule>,
    ) -> Self {
        Self::MulDiv(
            Box::new(value.into()),
            Box::new(multiplier.into()),
            Box::new(divisor.into()),
        )
    }
    pub fn new_div(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Div(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_mul(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Mul(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_add(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Add(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_sub(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Sub(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_mod(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Mod(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_min(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Min(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_max(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Max(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_lt(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Lt(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_lte(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Lte(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_gt(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Gt(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_gte(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Gte(Box::new(lhs.into()), Box::new(rhs.into()))
    }
    pub fn new_if(condition: impl Into<Rule>, then: impl Into<Rule>) -> Self {
        Self::If(Box::new(condition.into()), Box::new(then.into()))
    }

    pub fn new_if_not(condition: impl Into<Rule>, then: impl Into<Rule>) -> Self {
        Self::IfNot(Box::new(condition.into()), Box::new(then.into()))
    }

    pub fn new_if_else(
        condition: impl Into<Rule>,
        then: impl Into<Rule>,
        otherwise: impl Into<Rule>,
    ) -> Self {
        Self::IfElse(
            Box::new(condition.into()),
            Box::new(then.into()),
            Box::new(otherwise.into()),
        )
    }

    pub fn new_and(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::And(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_or(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Or(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_xor(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Xor(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_not(statement: impl Into<Rule>) -> Self {
        Self::Not(Box::new(statement.into()))
    }

    pub fn new_intersects(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Intersects(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_in(array: impl Into<Rule>, value: impl Into<Rule>) -> Self {
        Self::In(Box::new(array.into()), Box::new(value.into()))
    }

    pub fn new_nin(array: impl Into<Rule>, value: impl Into<Rule>) -> Self {
        Self::Nin(Box::new(array.into()), Box::new(value.into()))
    }

    pub fn new_between(
        start: impl Into<Rule>,
        end: impl Into<Rule>,
        value: impl Into<Rule>,
    ) -> Self {
        Self::Between(
            Box::new(start.into()),
            Box::new(end.into()),
            Box::new(value.into()),
        )
    }

    pub fn new_eq(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Eq(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_neq(lhs: impl Into<Rule>, rhs: impl Into<Rule>) -> Self {
        Self::Neq(Box::new(lhs.into()), Box::new(rhs.into()))
    }

    pub fn new_split(string: impl Into<Rule>, separator: impl Into<Rule>) -> Self {
        Self::Split(Box::new(string.into()), Box::new(separator.into()))
    }

    pub fn new_starts_with(string: impl Into<Rule>, start: impl Into<Rule>) -> Self {
        Self::StartsWith(Box::new(string.into()), Box::new(start.into()))
    }

    pub fn new_ends_with(string: impl Into<Rule>, end: impl Into<Rule>) -> Self {
        Self::EndsWith(Box::new(string.into()), Box::new(end.into()))
    }

    pub fn new_at(array: impl Into<Rule>, position: impl Into<Rule>) -> Self {
        Self::At(Box::new(array.into()), Box::new(position.into()))
    }

    pub fn new_only_show_if(condition: impl Into<Rule>) -> Self {
        Self::OnlyShowIf(Box::new(condition.into()))
    }

    pub fn new_do(rule: impl Into<Rule>) -> Self {
        Self::Do(Box::new(rule.into()))
    }

    pub fn new_get(key: &str) -> Self {
        Self::Get(key.to_string())
    }

    pub fn new_set(key: &str, eval: impl Into<Rule>) -> Self {
        Self::Set(key.to_string(), Box::new(eval.into()))
    }

    pub fn new_bn(value: impl Into<Value>) -> Self {
        Self::Bn(value.into())
    }

    pub fn new_get_price_in_usd(amount: impl Into<Rule>) -> Self {
        Self::GetPriceInUsd(Box::new(amount.into()))
    }
}

impl Value {
    pub fn try_bool(self) -> Result<bool, Error> {
        match self {
            Self::Bool(b) => Ok(b),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_string(self) -> Result<String, Error> {
        match self {
            Self::String(s) => Ok(s),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Self::Array(array) => Ok(array),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_bignum(self) -> Result<BigNum, Error> {
        BigNum::try_from(self)
    }

    pub fn try_number(self) -> Result<Number, Error> {
        match self {
            Value::Number(number) => Ok(number),
            _ => Err(Error::TypeError),
        }
    }
}

impl TryFrom<Value> for BigNum {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(string) => BigNum::from_str(&string).map_err(|_| Error::TypeError),
            Value::BigNum(big_num) => Ok(big_num),
            Value::Number(number) => {
                BigNum::from_str(&number.to_string()).map_err(|_| Error::TypeError)
            }
            _ => Err(Error::TypeError),
        }
    }
}

/// Evaluates a Rule to be applied and has 3 outcomes:
/// - Does nothing
///     Rules returned directly:
///     - Bool
///     - Number
///     - String
///     - Array
///     - BigNum
/// - Mutates output
/// - Throws an error
fn eval(input: &Input, output: &mut Output, rule: &Rule) -> Result<Option<Value>, Error> {
    let function = match rule {
        Rule::Value(value) => return Ok(Some(value.clone())),
        Rule::Function(function) => function,
    };

    // basic operators
    let value = match function {
        Function::MulDiv(first_rule, second_rule, third_rule) => {
            let product = Function::Mul(first_rule.clone(), second_rule.clone())
                .eval(input, output)?
                .ok_or(Error::TypeError)?;
            let product_rule = Rule::Value(product);
            let boxed_rule = Box::new(product_rule);
            Function::Div(boxed_rule, third_rule.clone()).eval(input, output)?
        }
        Function::Div(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), second_value) => {
                    let second_bignum = BigNum::try_from(second_value)?;

                    Value::BigNum(bignum.div(second_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.div(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Division)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Mul(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.mul(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.mul(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Multiplication)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Mod(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.rem(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.rem(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Modulus)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Add(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.add(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.add(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Addition)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Sub(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.sub(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.sub(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Subtraction)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Max(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.max(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.max(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Max)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Min(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::BigNum(bignum.min(rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::BigNum(lhs_bignum.min(rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Number(math_operator(lhs, rhs, MathOperator::Min)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::If(first_rule, second_rule) => {
            let eval_if = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            if eval_if {
                eval(input, output, second_rule)?
            } else {
                None
            }
        }
        Function::IfNot(if_rule, else_rule) => {
            let eval_if = eval(input, output, if_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            if !eval_if {
                eval(input, output, else_rule)?
            } else {
                None
            }
        }
        Function::IfElse(if_rule, then_rule, else_rule) => {
            let eval_if = eval(input, output, if_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            if eval_if {
                eval(input, output, then_rule)?
            } else {
                eval(input, output, else_rule)?
            }
        }
        Function::And(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(a && b))
        }
        Function::Or(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(a || b))
        }
        Function::Xor(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(a ^ b))
        }
        Function::Not(first_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(!a))
        }
        Function::Lt(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::Bool(bignum.lt(&rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::Bool(lhs_bignum.lt(&rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Bool(compare_numbers(lhs, rhs, ComparisonOperator::Lt)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Lte(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::Bool(bignum.le(&rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::Bool(lhs_bignum.le(&rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Bool(compare_numbers(lhs, rhs, ComparisonOperator::Lte)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Gt(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::Bool(bignum.gt(&rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::Bool(lhs_bignum.gt(&rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Bool(compare_numbers(lhs, rhs, ComparisonOperator::Gt)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Gte(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::Bool(bignum.ge(&rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::Bool(lhs_bignum.ge(&rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Bool(compare_numbers(lhs, rhs, ComparisonOperator::Gte)?)
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Eq(first_rule, second_rule) => {
            let first_eval = first_rule.eval(input, output)?.ok_or(Error::TypeError)?;
            let second_eval = second_rule.eval(input, output)?.ok_or(Error::TypeError)?;

            let value = match (first_eval, second_eval) {
                (Value::BigNum(bignum), rhs_value) => {
                    let rhs_bignum = BigNum::try_from(rhs_value)?;

                    Value::Bool(bignum.eq(&rhs_bignum))
                }
                (lhs_value, Value::BigNum(rhs_bignum)) => {
                    let lhs_bignum = BigNum::try_from(lhs_value)?;

                    Value::Bool(lhs_bignum.eq(&rhs_bignum))
                }
                (Value::Number(lhs), Value::Number(rhs)) => {
                    Value::Bool(compare_numbers(lhs, rhs, ComparisonOperator::Eq)?)
                }
                (Value::Bool(lhs), Value::Bool(rhs)) => Value::Bool(lhs == rhs),
                (Value::String(lhs), Value::String(rhs)) => Value::Bool(lhs == rhs),
                (Value::Array(lhs), Value::Array(rhs)) => {
                    if lhs.len() != rhs.len() {
                        Value::Bool(false)
                    } else {
                        let are_same = lhs.iter().zip(rhs.iter()).all(|(a, b)| a == b);
                        Value::Bool(are_same)
                    }
                }
                _ => return Err(Error::TypeError),
            };

            Some(value)
        }
        Function::Neq(first_rule, second_rule) => {
            let is_equal = Function::Eq(first_rule.clone(), second_rule.clone())
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            Some(Value::Bool(!is_equal))
        }
        Function::Intersects(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;

            Some(Value::Bool(a.iter().any(|x| b.contains(x))))
        }
        Function::In(array_value, search_value) => {
            let a = eval(input, output, array_value)?
                .ok_or(Error::TypeError)?
                .try_array()?;
            let b = eval(input, output, search_value)?.ok_or(Error::TypeError)?;

            Some(Value::Bool(a.contains(&b)))
        }
        Function::Nin(array_value, search_value) => {
            let is_in = Function::In(array_value.clone(), search_value.clone())
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            Some(Value::Bool(!is_in))
        }
        Function::Between(min_rule, max_rule, value_rule) => {
            let is_gte_start = Function::Gte(value_rule.clone(), min_rule.clone())
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            let is_lte_end = Function::Lte(value_rule.clone(), max_rule.clone())
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            Some(Value::Bool(is_gte_start && is_lte_end))
        }
        Function::At(array_rule, index_rule) => {
            let mut array_value = array_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_array()?;
            let index_value = index_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_number()?
                .as_u64()
                .ok_or(Error::TypeError)?;
            let index = usize::try_from(index_value).map_err(|_| Error::TypeError)?;

            if array_value.get(index).is_none() {
                return Err(Error::TypeError);
            } else {
                Some(array_value.swap_remove(index))
            }
        }
        Function::Split(string_rule, pattern_rule) => {
            let string_value = string_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;
            let pattern_value = pattern_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;

            let after_split = string_value
                .split(&pattern_value)
                .map(Value::new_string)
                .collect();

            Some(Value::Array(after_split))
        }
        Function::StartsWith(string_rule, starts_with_rule) => {
            let string_value = string_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;
            let starts_with_value = starts_with_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;

            Some(Value::Bool(string_value.starts_with(&starts_with_value)))
        }
        Function::EndsWith(string_rule, ends_with_rule) => {
            let string_value = string_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;
            let ends_with_value = ends_with_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_string()?;

            Some(Value::Bool(string_value.ends_with(&ends_with_value)))
        }
        Function::OnlyShowIf(rule) => {
            let eval = rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bool()?;
            let new_rule = Box::new(Rule::Value(Value::Bool(eval)));

            Function::Set(String::from("show"), new_rule).eval(input, output)?
        }
        Function::GetPriceInUsd(amount_rule) => {
            let amount = amount_rule
                .eval(input, output)?
                .ok_or(Error::TypeError)?
                .try_bignum()?;

            // if there is no way to get the deposit_asset, then fail with UnknownVariable
            // since we can't calculate the price in USD
            let deposit_asset = match &input.channel {
                Some(Get::Getter(ChannelGetter::Full(full_channel))) => {
                    Ok(full_channel.channel.deposit_asset.clone())
                }
                Some(Get::Getter(ChannelGetter::Market(channel))) => {
                    Ok(channel.deposit_asset.clone())
                }
                // In case of a Values - we don't have the deposit_asset on hand so we fail in that case
                // In case of None we also fail
                _ => Err(Error::UnknownVariable),
            }?;

            let divisor = DEPOSIT_ASSETS_MAP
                .get(&deposit_asset)
                .ok_or(Error::TypeError)?;
            let amount_in_usd = amount.div(divisor).to_f64().ok_or(Error::TypeError)?;
            let amount_as_number = Number::from_f64(amount_in_usd).ok_or(Error::TypeError)?;
            Some(Value::Number(amount_as_number))
        }
        Function::Do(first_rule) => eval(input, output, first_rule)?,
        Function::Set(key, rule) => {
            // Output variables can be set any number of times by different rules, except `show`
            // if `show` is at any point set to `false`, we stop executing rules and don't show the ad.
            match key.as_str() {
                "boost" => {
                    let boost_num = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_number()?;

                    output.boost = boost_num.as_f64().ok_or(Error::TypeError)?;
                }
                "show" => {
                    let show_value = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bool()?;

                    output.show = show_value;
                }
                "price.IMPRESSION" => {
                    let price = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bignum()?;

                    // we do not care about any other old value
                    output.price.insert("IMPRESSION".to_string(), price);
                }
                "price.CLICK" => {
                    let price = rule
                        .eval(input, output)?
                        .ok_or(Error::TypeError)?
                        .try_bignum()?;

                    // we do not care about any other old value
                    output.price.insert("CLICK".to_string(), price);
                }
                _ => return Err(Error::UnknownVariable),
            }

            return Ok(None);
        }
        Function::Get(key) => match input.try_get(key) {
            Ok(value) => Some(value),
            Err(Error::UnknownVariable) => Some(output.try_get(key)?),
            Err(e) => return Err(e),
        },
        Function::Bn(value) => {
            let big_num = value.clone().try_bignum()?;

            Some(Value::BigNum(big_num))
        }
    };

    Ok(value)
}

/// Stops (i.e. it short-circuits) evaluating `Rule`s when `Output.show` becomes `false`
pub fn eval_multiple(
    rules: &[Rule],
    input: &Input,
    output: &mut Output,
) -> Vec<Result<Option<Value>, (Error, Rule)>> {
    let mut results = vec![];

    for rule in rules {
        results.push(rule.eval(input, output).map_err(|err| (err, rule.clone())));

        if !output.show {
            break;
        }
    }

    results
}

pub fn eval_with_callback<F: Fn(Error, Rule)>(
    rules: &[Rule],
    input: &Input,
    output: &mut Output,
    on_type_error: Option<F>,
) {
    for result in eval_multiple(rules, input, output) {
        match (result, on_type_error.as_ref()) {
            (Ok(_), _) => {}
            (Err((Error::UnknownVariable, _)), _) => {}
            (Err((Error::TypeError, rule)), Some(on_type_error)) => {
                on_type_error(Error::TypeError, rule)
            }
            // skip any other case, including Error::TypeError if there is no passed function
            _ => {}
        }

        if !output.show {
            return;
        }
    }
}

enum MathOperator {
    Division,
    Multiplication,
    Modulus,
    Addition,
    Subtraction,
    Max,
    Min,
}

enum ComparisonOperator {
    /// First value is greater than second value
    Gt,
    /// First value is greater than or equal to second value
    Gte,
    /// First value is lesser than second value
    Lt,
    /// First value is lesser than or equal to second value
    Lte,
    /// Values are equal
    Eq,
}

fn compare_numbers(lhs: Number, rhs: Number, ops: ComparisonOperator) -> Result<bool, Error> {
    match (lhs.as_u64(), rhs.as_u64()) {
        (Some(lhs), Some(rhs)) => Ok(handle_comparisons(lhs, rhs, ops)),
        _ => match (lhs.as_i64(), rhs.as_i64()) {
            (Some(lhs), Some(rhs)) => Ok(handle_comparisons(lhs, rhs, ops)),
            _ => match (lhs.as_f64(), rhs.as_f64()) {
                (Some(lhs), Some(rhs)) => Ok(handle_comparisons(lhs, rhs, ops)),
                _ => Err(Error::TypeError),
            },
        },
    }
}

fn handle_comparisons<T: PartialOrd>(lhs: T, rhs: T, ops: ComparisonOperator) -> bool {
    match ops {
        ComparisonOperator::Lt => lhs.lt(&rhs),
        ComparisonOperator::Lte => lhs.le(&rhs),
        ComparisonOperator::Gt => lhs.gt(&rhs),
        ComparisonOperator::Gte => lhs.ge(&rhs),
        ComparisonOperator::Eq => lhs.eq(&rhs),
    }
}

fn handle_u64(lhs: u64, rhs: u64, ops: MathOperator) -> Result<Number, Error> {
    match ops {
        MathOperator::Division => {
            let divided = lhs.checked_div(rhs).ok_or(Error::TypeError)?;
            Ok(divided.into())
        }
        MathOperator::Multiplication => {
            let multiplied = lhs.checked_mul(rhs).ok_or(Error::TypeError)?;
            Ok(multiplied.into())
        }
        MathOperator::Modulus => {
            let modulus = lhs.checked_rem(rhs).ok_or(Error::TypeError)?;
            Ok(modulus.into())
        }
        MathOperator::Addition => {
            let added = lhs.checked_add(rhs).ok_or(Error::TypeError)?;
            Ok(added.into())
        }
        MathOperator::Subtraction => {
            let subtracted = lhs.checked_sub(rhs).ok_or(Error::TypeError)?;
            Ok(subtracted.into())
        }
        MathOperator::Max => {
            let max = lhs.max(rhs);
            Ok(max.into())
        }
        MathOperator::Min => {
            let min = lhs.min(rhs);
            Ok(min.into())
        }
    }
}

fn handle_i64(lhs: i64, rhs: i64, ops: MathOperator) -> Result<Number, Error> {
    match ops {
        MathOperator::Division => {
            let divided = lhs.checked_div(rhs).ok_or(Error::TypeError)?;
            Ok(divided.into())
        }
        MathOperator::Multiplication => {
            let multiplied = lhs.checked_mul(rhs).ok_or(Error::TypeError)?;
            Ok(multiplied.into())
        }
        MathOperator::Modulus => {
            let modulus = lhs.checked_rem(rhs).ok_or(Error::TypeError)?;
            Ok(modulus.into())
        }
        MathOperator::Addition => {
            let added = lhs.checked_add(rhs).ok_or(Error::TypeError)?;
            Ok(added.into())
        }
        MathOperator::Subtraction => {
            let subtracted = lhs.checked_sub(rhs).ok_or(Error::TypeError)?;
            Ok(subtracted.into())
        }
        MathOperator::Max => {
            let max = lhs.max(rhs);
            Ok(max.into())
        }
        MathOperator::Min => {
            let min = lhs.min(rhs);
            Ok(min.into())
        }
    }
}

fn handle_f64(lhs: f64, rhs: f64, ops: MathOperator) -> Result<Number, Error> {
    match ops {
        MathOperator::Division => {
            let divided = lhs.div(rhs);
            Ok(Number::from_f64(divided).ok_or(Error::TypeError)?)
        }
        MathOperator::Multiplication => {
            let multiplied = lhs.mul(rhs);
            Ok(Number::from_f64(multiplied).ok_or(Error::TypeError)?)
        }
        MathOperator::Modulus => {
            let modulus = lhs.rem(rhs);
            Ok(Number::from_f64(modulus).ok_or(Error::TypeError)?)
        }
        MathOperator::Addition => {
            let added = lhs.add(rhs);
            Ok(Number::from_f64(added).ok_or(Error::TypeError)?)
        }
        MathOperator::Subtraction => {
            let subtracted = lhs.sub(rhs);
            Ok(Number::from_f64(subtracted).ok_or(Error::TypeError)?)
        }
        MathOperator::Max => {
            let max = lhs.max(rhs);
            Ok(Number::from_f64(max).ok_or(Error::TypeError)?)
        }
        MathOperator::Min => {
            let min = lhs.min(rhs);
            Ok(Number::from_f64(min).ok_or(Error::TypeError)?)
        }
    }
}

fn math_operator(lhs: Number, rhs: Number, ops: MathOperator) -> Result<Number, Error> {
    match (lhs.as_u64(), rhs.as_u64()) {
        (Some(lhs), Some(rhs)) => handle_u64(lhs, rhs, ops),
        _ => match (lhs.as_i64(), rhs.as_i64()) {
            (Some(lhs), Some(rhs)) => handle_i64(lhs, rhs, ops),
            _ => match (lhs.as_f64(), rhs.as_f64()) {
                (Some(lhs), Some(rhs)) => handle_f64(lhs, rhs, ops),
                _ => Err(Error::TypeError),
            },
        },
    }
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::*;
    use bytes::BytesMut;
    use postgres_types::{accepts, to_sql_checked, IsNull, Json, ToSql, Type};
    use std::error::Error;

    impl ToSql for Rules {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }
}
