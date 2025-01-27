use json;
use std::{fs::File,
          io::Read,
          path::PathBuf};
use valico::json_schema;

/// Asserts that `json_string` is valid according to the specified JSON schema.
///
/// In the case of invalid input, all validation errors are neatly
/// rendered, along with a pretty-printed formatting of the
/// offending JSON.
pub fn assert_valid(json_string: &str, schema: &str) {
    let result = validate_string(json_string, schema);
    if !result.is_valid() {
        let error_string = result.errors
                                 .into_iter()
                                 .map(|x| format!("  {:?}", x))
                                 .collect::<Vec<String>>()
                                 .join("\n");
        let pretty_json = json::stringify_pretty(json::parse(json_string).expect("JSON should \
                                                                                  parse if we \
                                                                                  get this far"),
                                                 2);
        panic!(
               r#"
JSON does not validate!
Errors:
{}

JSON:
{}
"#,
               error_string, pretty_json
        );
    }
}

/// Compares the incoming JSON string against the JSON schema
/// and returns the resulting `ValidationState`.
///
/// In general, you should prefer using `assert_valid` directly.
pub fn validate_string(input: &str, schema: &str) -> json_schema::ValidationState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("doc")
                                                        .join(schema);
    let mut schema_file = File::open(path).expect("could not open schema file");
    let mut raw_schema = String::new();
    schema_file.read_to_string(&mut raw_schema)
               .expect("could not read schema file");
    let parsed_schema: serde_json::Value =
        serde_json::from_str(&raw_schema).expect("Could not parse schema as JSON");
    let mut scope = json_schema::scope::Scope::new();
    // NOTE: using `false` instead of `true` allows us to use
    // `$comment` keyword, as well as our own `$deprecated` and
    // `$since` keywords.
    let schema = scope.compile_and_return(parsed_schema, false)
                      .expect("Could not compile the schema");

    let input_json: serde_json::Value =
        serde_json::from_str(input).expect("Could not parse input as JSON");
    schema.validate(&input_json)
}
