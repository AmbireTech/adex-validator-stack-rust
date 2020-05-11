use crate::BigNum;
use serde_json::{
    value::{Map as SerdeMap, Value as SerdeValue},
    Number,
};
use std::convert::TryFrom;

pub type Map = SerdeMap<String, SerdeValue>;
pub type Rule = SerdeValue;

pub enum Error {
    TypeError,
    UnknownVariable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalValue {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<EvalValue>),
    BigNum(BigNum),
}

impl TryFrom<SerdeValue> for EvalValue {
    type Error = Error;

    fn try_from(serde_value: SerdeValue) -> Result<Self, Self::Error> {
        match serde_value {
            SerdeValue::Null => Ok(Self::Null),
            SerdeValue::Bool(bool) => Ok(Self::Bool(bool)),
            SerdeValue::Number(number) => Ok(Self::Number(number)),
            SerdeValue::String(string) => Ok(Self::String(string)),
            SerdeValue::Array(serde_array) => {
                let array = serde_array
                    .into_iter()
                    .map(EvalValue::try_from)
                    .collect::<Result<_, _>>()?;
                Ok(Self::Array(array))
            }
            SerdeValue::Object(_) => Err(Error::TypeError),
        }
    }
}

impl EvalValue {
    pub fn try_bool(&self) -> Result<bool, Error> {
        match *self {
            Self::Bool(b) => Ok(b),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_array(&self) -> Result<Vec<EvalValue>, Error> {
        match *self {
            Self::Array(ref array) => Ok(array.to_vec()),
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
/// - Mutates output
/// - Throws an error
// TODO: Move to own module!
pub fn eval(input: &Map, output: &mut Map, rule: &Rule) -> Result<EvalValue, Error> {
    let rule = match rule {
        Rule::Null => return Err(Error::TypeError),
        Rule::Object(map) => map,
        value => return EvalValue::try_from(value.to_owned()),
    };

    // basic operators
    if let Some(SerdeValue::Array(array)) = rule.get("if") {
        let (first_rule, second_rule) = match array.get(0..=1) {
            Some(&[ref first_rule, ref second_rule]) => (first_rule, second_rule),
            _ => return Err(Error::TypeError),
        };

        let eval_if = eval(input, output, first_rule)?.try_bool()?;

        if eval_if {
            let bool = eval(input, output, second_rule)?.try_bool()?;
            return Ok(EvalValue::Bool(bool));
        }
    } else if let Some(SerdeValue::Array(array)) = rule.get("intersects") {
        // lists
        let (first_rule, second_rule) = match array.get(0..=1) {
            Some(&[ref first_rule, ref second_rule]) => (first_rule, second_rule),
            _ => return Err(Error::TypeError),
        };

        let a = eval(input, output, first_rule)?.try_array()?;
        let b = eval(input, output, second_rule)?.try_array()?;

        return Ok(EvalValue::Bool(a.iter().any(|x| b.contains(x))));
    }

    Ok(EvalValue::Null)
}
