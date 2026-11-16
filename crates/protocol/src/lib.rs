#![cfg_attr(not(any(test, feature = "std")), no_std)]

pub const ACK_BYTE: u8 = 0xA5;
pub const PACKET_LEN: usize = 8;
pub type Packet = [u8; PACKET_LEN];

#[repr(u8)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapModel {
    A100F = 0,
    B350F = 1,
    C500F = 2,
}

impl CapModel {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::A100F),
            1 => Some(Self::B350F),
            2 => Some(Self::C500F),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(tag = "test_type", rename_all = "snake_case")
)]
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
    pub fn tag(&self) -> u8 {
        match self {
            Self::Esr { .. } => 1,
            Self::Capacitance { .. } => 2,
            Self::Leakage { .. } => 3,
        }
    }

    pub fn encode(&self) -> Packet {
        let mut packet = Packet::default();
        packet[0] = self.tag();

        match self {
            TestCommand::Esr {
                cap_model,
                pulse_ma,
                pulse_ms,
            } => {
                packet[1] = cap_model.as_u8();
                packet[2..4].copy_from_slice(&pulse_ma.to_le_bytes());
                packet[4..6].copy_from_slice(&pulse_ms.to_le_bytes());
            }
            TestCommand::Capacitance {
                cap_model,
                charge_ma,
                max_mv,
                hold_sec,
            } => {
                packet[1] = cap_model.as_u8();
                packet[2..4].copy_from_slice(&charge_ma.to_le_bytes());
                packet[4..6].copy_from_slice(&max_mv.to_le_bytes());
                packet[6..8].copy_from_slice(&hold_sec.to_le_bytes());
            }
            TestCommand::Leakage {
                cap_model,
                test_mv,
                soak_sec,
            } => {
                packet[1] = cap_model.as_u8();
                packet[2..4].copy_from_slice(&test_mv.to_le_bytes());
                packet[4..6].copy_from_slice(&soak_sec.to_le_bytes());
            }
        }
        packet
    }

    pub fn decode(bytes: Packet) -> Result<Self, DecodeError> {
        let cap_model =
            CapModel::from_u8(bytes[1]).ok_or(DecodeError::UnknownCapModel(bytes[1]))?;

        let read_u16 = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);

        match bytes[0] {
            1 => Ok(Self::Esr {
                cap_model,
                pulse_ma: read_u16(2),
                pulse_ms: read_u16(4),
            }),
            2 => Ok(Self::Capacitance {
                cap_model,
                charge_ma: read_u16(2),
                max_mv: read_u16(4),
                hold_sec: read_u16(6),
            }),
            3 => Ok(Self::Leakage {
                cap_model,
                test_mv: read_u16(2),
                soak_sec: read_u16(4),
            }),
            other => Err(DecodeError::UnknownTestType(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownTestType(u8),
    UnknownCapModel(u8),
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match *self {
            Self::UnknownTestType(tag) => write!(f, "unknown test type tag: {tag}"),
            Self::UnknownCapModel(model) => write!(f, "unknown cap model: {model}"),
        }
    }
}

impl core::error::Error for DecodeError {}

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

    const ESR_BYTES: Packet = [1, 0, 0x10, 0x27, 0x91, 0x05, 0, 0];
    const CAPACITANCE_BYTES: Packet = [2, 1, 0x0F, 0x00, 0x0F, 0x00, 0x0C, 0x00];
    const LEAKAGE_BYTES: Packet = [3, 2, 0xC4, 0x09, 0x3C, 0x00, 0, 0];

    const ENDIAN_ESR: TestCommand = TestCommand::Esr {
        cap_model: CapModel::A100F,
        pulse_ma: 0x1234,
        pulse_ms: 0x5678,
    };
    const ENDIAN_PACKET: Packet = [1, 0, 0x34, 0x12, 0x78, 0x56, 0, 0];

    const INVALID_TEST_TYPES: [u8; 3] = [0, 4, u8::MAX];
    const INVALID_CAP_MODELS: [u8; 2] = [3, u8::MAX];

    #[test]
    fn packet_len_is_stable() {
        assert_eq!(PACKET_LEN, 8);
    }

    #[test]
    fn cap_model_maps_to_wire_bytes() {
        assert_eq!(CapModel::A100F.as_u8(), 0);
        assert_eq!(CapModel::B350F.as_u8(), 1);
        assert_eq!(CapModel::C500F.as_u8(), 2);
    }

    #[test]
    fn cap_model_round_trips() {
        for model in [CapModel::A100F, CapModel::B350F, CapModel::C500F] {
            assert_eq!(CapModel::from_u8(model.as_u8()), Some(model));
        }
    }

    #[test]
    fn cap_model_rejects_unknown_bytes() {
        for model in INVALID_CAP_MODELS {
            assert_eq!(CapModel::from_u8(model), None);
        }
    }

    #[test]
    fn tag_matches_variant() {
        assert_eq!(ESR.tag(), 1);
        assert_eq!(CAPACITANCE.tag(), 2);
        assert_eq!(LEAKAGE.tag(), 3);
    }

    #[test]
    fn encodes_golden_packets() {
        assert_eq!(ESR.encode(), ESR_BYTES);
        assert_eq!(CAPACITANCE.encode(), CAPACITANCE_BYTES);
        assert_eq!(LEAKAGE.encode(), LEAKAGE_BYTES);
    }

    #[test]
    fn encode_zeroes_unused_trailing_bytes() {
        for command in [ESR, LEAKAGE] {
            assert_eq!(command.encode()[6..], [0, 0]);
        }
    }

    #[test]
    fn round_trips_all_variants() {
        for command in ALL_COMMANDS {
            assert_eq!(TestCommand::decode(command.encode()), Ok(command));
        }
    }

    #[test]
    fn round_trips_extreme_values() {
        for command in EXTREME_COMMANDS {
            assert_eq!(TestCommand::decode(command.encode()), Ok(command));
        }
    }

    #[test]
    fn decodes_little_endian() {
        assert_eq!(TestCommand::decode(ENDIAN_PACKET), Ok(ENDIAN_ESR));
    }

    #[test]
    fn decode_rejects_unknown_test_type() {
        for tag in INVALID_TEST_TYPES {
            let mut bytes = ESR_BYTES;
            bytes[0] = tag;
            assert_eq!(
                TestCommand::decode(bytes),
                Err(DecodeError::UnknownTestType(tag))
            );
        }
    }

    #[test]
    fn decode_rejects_unknown_cap_model() {
        for model in INVALID_CAP_MODELS {
            let mut bytes = ESR_BYTES;
            bytes[1] = model;
            assert_eq!(
                TestCommand::decode(bytes),
                Err(DecodeError::UnknownCapModel(model))
            );
        }
    }

    #[test]
    fn decode_ignores_unused_trailing_bytes() {
        for (command, mut bytes) in [(ESR, ESR_BYTES), (LEAKAGE, LEAKAGE_BYTES)] {
            bytes[6] = 0xFF;
            bytes[7] = 0xFF;
            assert_eq!(TestCommand::decode(bytes), Ok(command));
        }
    }

    #[test]
    fn decode_error_display() {
        assert_eq!(
            DecodeError::UnknownTestType(9).to_string(),
            "unknown test type tag: 9"
        );
        assert_eq!(
            DecodeError::UnknownCapModel(9).to_string(),
            "unknown cap model: 9"
        );
    }

    #[test]
    fn decode_error_implements_error() {
        fn assert_error<E: core::error::Error>() {}
        assert_error::<DecodeError>();
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
