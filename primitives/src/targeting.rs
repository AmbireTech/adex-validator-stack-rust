use crate::BigNum;
use serde::{Deserialize, Serialize};
use serde_json::{
    value::{Map as SerdeMap, Value as SerdeValue},
    Number,
};
use std::convert::TryFrom;
use std::fmt;

pub type Map = SerdeMap<String, SerdeValue>;

#[derive(Debug)]
pub enum Error {
    TypeError,
    UnknownVariable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TypeError => write!(f, "TypeError: Wrong type"),
            Error::UnknownVariable => write!(f, "UnknownVariable: Unknown varialbe passed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "SerdeValue")]
pub enum Value {
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    BigNum(BigNum),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Rule {
    Function(Function),
    Value(Value),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Function {
    If(Box<Rule>, Box<Rule>),
    And(Box<Rule>, Box<Rule>),
    Intersects(Box<Rule>, Box<Rule>),
    Get(String),
}

impl TryFrom<SerdeValue> for Value {
    type Error = Error;

    fn try_from(serde_value: SerdeValue) -> Result<Self, Self::Error> {
        match serde_value {
            SerdeValue::Bool(bool) => Ok(Self::Bool(bool)),
            SerdeValue::Number(number) => Ok(Self::Number(number)),
            SerdeValue::String(string) => Ok(Self::String(string)),
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

impl Value {
    pub fn try_bool(self) -> Result<bool, Error> {
        match self {
            Self::Bool(b) => Ok(b),
            _ => Err(Error::TypeError),
        }
    }

    pub fn try_array(self) -> Result<Vec<Value>, Error> {
        match self {
            Self::Array(array) => Ok(array),
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
pub fn eval(input: &Map, output: &mut Map, rule: &Rule) -> Result<Option<Value>, Error> {
    let function = match rule {
        Rule::Value(value) => return Ok(Some(value.clone())),
        Rule::Function(function) => function,
    };

    // basic operators
    let value = match function {
        Function::If(first_rule, second_rule) => {
            let eval_if = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_bool()?;

            if eval_if {
                let bool = eval(input, output, second_rule)?
                    .ok_or(Error::TypeError)?
                    .try_bool()?;
                Some(Value::Bool(bool))
            } else {
                None
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
        Function::Intersects(first_rule, second_rule) => {
            let a = eval(input, output, first_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;
            let b = eval(input, output, second_rule)?
                .ok_or(Error::TypeError)?
                .try_array()?;

            Some(Value::Bool(a.iter().any(|x| b.contains(x))))
        }
        Function::Get(key) => {
            let input_value = input.get(key).ok_or(Error::UnknownVariable)?;

            Some(Value::try_from(input_value.clone())?)
        }
    };

    Ok(value)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialzes_intersects_rule() {
        let json = r#"{"intersects": [{ "get": "adSlot.categories" }, ["News", "Bitcoin"]]}"#;

        let parsed_rule = serde_json::from_str::<Rule>(json).expect("Should deserialize");

        let mut expected_map = SerdeMap::new();
        expected_map.insert(
            "get".to_string(),
            SerdeValue::String("adSlot.categories".to_string()),
        );

        let expected = Rule::Function(Function::Intersects(
            Box::new(Rule::Function(Function::Get(
                "adSlot.categories".to_string(),
            ))),
            Box::new(Rule::Value(Value::Array(vec![
                Value::String("News".to_string()),
                Value::String("Bitcoin".to_string()),
            ]))),
        ));

        assert_eq!(expected, parsed_rule)
    }

    /// ```json
    /// {
    ///   "intersects": [
    ///     {
    ///       "get": "publisherId"
    ///     },
    ///     [
    ///       "0xd5860D6196A4900bf46617cEf088ee6E6b61C9d6",
    ///       "0xd5860D6196A4900bf46617cEf088ee6E6b61C9d3"
    ///     ]
    ///   ]
    /// }
    /// ```
    #[test]
    fn test_simple_intersect_eval() {
        let input: Map = vec![(
            "publisherId".to_string(),
            SerdeValue::Array(vec![SerdeValue::String(
                "0xd5860D6196A4900bf46617cEf088ee6E6b61C9d6".to_string(),
            )]),
        )]
        .into_iter()
        .collect();
        let mut output = SerdeMap::new();

        let publishers = vec![
            Value::String("0xd5860D6196A4900bf46617cEf088ee6E6b61C9d6".to_string()),
            Value::String("0xd5860D6196A4900bf46617cEf088ee6E6b61C9d3".to_string()),
        ];

        let rules = Rule::Function(Function::Intersects(
            Box::new(Rule::Function(Function::Get("publisherId".to_string()))),
            Box::new(Rule::Value(Value::Array(publishers))),
        ));

        let result = eval(&input, &mut output, &rules).expect("Should eval rules");

        assert_eq!(Value::Bool(true), result.expect("Sould be Some!"));
    }
}
