use machine_uid::get;
use snowdon::{Epoch, Generator, Layout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Retrieves a unique machine identifier and hashes it to fit into 10 bits.
fn generate_machine_id() -> u16 {
    // Retrieve the machine UID as a string
    let uid = get().expect("Failed to get machine UID");

    // Hash the UID to generate a consistent u16 value
    let mut hasher = DefaultHasher::new();
    uid.hash(&mut hasher);
    let hash = hasher.finish();

    // Truncate the hash to fit into 10 bits (0-1023)
    (hash & 0x3FF) as u16
}

/// Custom layout struct for Snowflake ID generation.
struct CustomLayout;

impl Layout for CustomLayout {
    fn construct_snowflake(timestamp: u64, sequence_number: u64) -> u64 {
        assert!(
            !Self::exceeds_timestamp(timestamp) && !Self::exceeds_sequence_number(sequence_number)
        );
        let machine_id = generate_machine_id() as u64;
        (timestamp << 22) | (machine_id << 12) | sequence_number
    }

    fn timestamp(input: u64) -> u64 {
        input >> 22
    }

    fn exceeds_timestamp(input: u64) -> bool {
        input >= (1 << 42)
    }

    fn sequence_number(input: u64) -> u64 {
        input & ((1 << 12) - 1)
    }

    fn exceeds_sequence_number(input: u64) -> bool {
        input >= (1 << 12)
    }

    fn is_valid_snowflake(_input: u64) -> bool {
        true // Implement validation logic if needed
    }
}

impl Epoch for CustomLayout {
    fn millis_since_unix() -> u64 {
        1_420_070_400_000 // Custom epoch (e.g., January 1, 2015)
    }
}

type CustomSnowflakeGenerator = Generator<CustomLayout, CustomLayout>;

/// Creates a new run identifier.
pub fn generate_run_id() -> String {
    let generator = CustomSnowflakeGenerator::default();
    let snowflake_id = generator.generate().expect("Failed to generate ID");
    let id_as_u64: u64 = snowflake_id.into_inner();
    format!("{:X}", id_as_u64)
}

/// Creates a new compound run identifier with a given prefix.
pub fn generate_compound_run_id(prefix: &str) -> String {
    format!("{}||{}", prefix, generate_run_id())
}

/// Strips the GUID from a compound run identifier.
pub fn strip_guid(string: &str) -> String {
    string.split("||").next().unwrap_or_default().to_string()
}
