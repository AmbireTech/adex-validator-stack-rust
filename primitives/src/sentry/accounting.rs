





// This modules implements the needed non-generic structs that help with Deserialization of the `Balances<S>`
// mod de {
//     use crate::balances::UncheckedState;

//     use super::*;

//     #[derive(Deserialize)]
//     struct DeserializeAccounting {
//         pub channel: Channel,
//         #[serde(flatten)]
//         pub balances: DeserializeBalances,
//         pub created: DateTime<Utc>,
//         pub updated: Option<DateTime<Utc>>,
//     }

//     impl<'de> Deserialize<'de> for Accounting<UncheckedState> {
//         fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//         where
//             D: Deserializer<'de>,
//         {
//             let de_acc = DeserializeAccounting::deserialize(deserializer)?;

//             Ok(Self {
//                 channel: de_acc.channel,
//                 balances: Balances::<UncheckedState>::try_from(de_acc.balances)
//                     .map_err(serde::de::Error::custom)?,
//                 created: de_acc.created,
//                 updated: de_acc.updated,
//             })
//         }
//     }

//     impl<'de> Deserialize<'de> for Accounting<CheckedState> {
//         fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//         where
//             D: Deserializer<'de>,
//         {
//             let unchecked_acc = Accounting::<UncheckedState>::deserialize(deserializer)?;

//             Ok(Self {
//                 channel: unchecked_acc.channel,
//                 balances: unchecked_acc
//                     .balances
//                     .check()
//                     .map_err(serde::de::Error::custom)?,
//                 created: unchecked_acc.created,
//                 updated: unchecked_acc.updated,
//             })
//         }
//     }

// }
