#![cfg_attr(not(any(test, feature = "std")), no_std)]

extern crate alloc;

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

pub const ACK_BYTE: u8 = 0xA5;

pub type Packet = Vec<u8>;
pub type EncodeError = ciborium::ser::Error<core::convert::Infallible>;
pub type DecodeError = ciborium::de::Error<ciborium_io::EndOfFile>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapModel {
    A100F,
    B350F,
    C500F,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "test_type", rename_all = "snake_case")]
pub enum TestCommand {
    Esr {
        cap_model: CapModel,
        pulse_ma: u16,
        pulse_ms: u16,
    },
    Capacitance {
        cap_model: CapModel,
        charge_ma: u16,
        max_mv: u16,
        hold_sec: u16,
    },
    Leakage {
        cap_model: CapModel,
        test_mv: u16,
        soak_sec: u16,
    },
}

impl TestCommand {
    pub fn encode(&self) -> Result<Packet, EncodeError> {
        let mut bytes = Packet::new();
        ciborium::into_writer(self, &mut bytes)?;
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut scratch = [0; 64];
        ciborium::from_reader_with_buffer(bytes, &mut scratch)
    }
}

#[cfg(feature = "json")]
impl TestCommand {
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESR: TestCommand = TestCommand::Esr {
        cap_model: CapModel::A100F,
        pulse_ma: 10_000,
        pulse_ms: 1_425,
    };

    const CAPACITANCE: TestCommand = TestCommand::Capacitance {
        cap_model: CapModel::B350F,
        charge_ma: 15,
        max_mv: 15,
        hold_sec: 12,
    };

    const LEAKAGE: TestCommand = TestCommand::Leakage {
        cap_model: CapModel::C500F,
        test_mv: 2_500,
        soak_sec: 60,
    };

    const ALL_COMMANDS: [TestCommand; 3] = [ESR, CAPACITANCE, LEAKAGE];

    const EXTREME_COMMANDS: [TestCommand; 3] = [
        TestCommand::Esr {
            cap_model: CapModel::A100F,
            pulse_ma: 0,
            pulse_ms: u16::MAX,
        },
        TestCommand::Capacitance {
            cap_model: CapModel::C500F,
            charge_ma: u16::MAX,
            max_mv: 0,
            hold_sec: u16::MAX,
        },
        TestCommand::Leakage {
            cap_model: CapModel::B350F,
            test_mv: u16::MAX,
            soak_sec: 0,
        },
    ];

    #[test]
    fn round_trips_all_variants() {
        for command in ALL_COMMANDS {
            let bytes = command.encode().unwrap();
            assert_eq!(TestCommand::decode(&bytes).unwrap(), command);
        }
    }

    #[test]
    fn round_trips_extreme_values() {
        for command in EXTREME_COMMANDS {
            let bytes = command.encode().unwrap();
            assert_eq!(TestCommand::decode(&bytes).unwrap(), command);
        }
    }

    #[test]
    fn decode_rejects_corrupted_input() {
        let mut bytes = ESR.encode().unwrap();
        bytes[0] = 0xFF;
        assert!(TestCommand::decode(&bytes).is_err());
    }

    #[test]
    fn decode_rejects_empty_input() {
        assert!(TestCommand::decode(&[]).is_err());
    }

    #[cfg(feature = "json")]
    mod json_tests {
        use super::super::*;
        use super::{CAPACITANCE, ESR, LEAKAGE};

        const ESR_JSON: &str =
            r#"{"test_type":"esr","cap_model":"A100F","pulse_ma":10000,"pulse_ms":1425}"#;
        const CAPACITANCE_JSON: &str = r#"{"test_type":"capacitance","cap_model":"B350F","charge_ma":15,"max_mv":15,"hold_sec":12}"#;
        const LEAKAGE_JSON: &str =
            r#"{"test_type":"leakage","cap_model":"C500F","test_mv":2500,"soak_sec":60}"#;

        const INVALID_JSON: [&str; 5] = [
            r#"{"test_type":"nope","cap_model":"A100F","pulse_ma":1,"pulse_ms":2}"#,
            r#"{"test_type":"esr","cap_model":"model_a_100f","pulse_ma":1,"pulse_ms":2}"#,
            r#"{"test_type":"esr","cap_model":"A100F","pulse_ma":1}"#,
            r#"{"test_type":"esr","cap_model":"A100F","pulse_ma":70000,"pulse_ms":2}"#,
            r#"[]"#,
        ];

        #[test]
        fn parses_each_variant() {
            assert_eq!(TestCommand::from_json(ESR_JSON).unwrap(), ESR);
            assert_eq!(
                TestCommand::from_json(CAPACITANCE_JSON).unwrap(),
                CAPACITANCE
            );
            assert_eq!(TestCommand::from_json(LEAKAGE_JSON).unwrap(), LEAKAGE);
        }

        #[test]
        fn serializes_flat_tagged_json() {
            let value: serde_json::Value = serde_json::from_str(&ESR.to_json().unwrap()).unwrap();

            assert_eq!(value["test_type"].as_str(), Some("esr"));
            assert_eq!(value["cap_model"].as_str(), Some("A100F"));
            assert_eq!(value["pulse_ma"].as_u64(), Some(10_000));
            assert_eq!(value["pulse_ms"].as_u64(), Some(1_425));
            assert!(value.get("Esr").is_none());
        }

        #[test]
        fn round_trips_json() {
            for command in [ESR, CAPACITANCE, LEAKAGE] {
                let json = command.to_json().unwrap();
                assert_eq!(TestCommand::from_json(&json).unwrap(), command);
            }
        }

        #[test]
        fn rejects_invalid_json() {
            for json in INVALID_JSON {
                assert!(TestCommand::from_json(json).is_err(), "accepted: {json}");
            }
        }
    }
}
