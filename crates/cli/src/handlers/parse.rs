use {
    crate::{
        accounts::{
            legacy_process_accounts, process_accounts, AccountsModTemplate, AccountsStructTemplate,
        },
        events::{legacy_process_events, process_events, EventsStructTemplate},
        instructions::{
            legacy_process_instructions, process_instructions, InstructionsModTemplate,
            InstructionsStructTemplate,
        },
        types::{legacy_process_types, process_types, TypeStructTemplate},
        util::{is_big_array, legacy_read_idl, read_idl},
    },
    anyhow::{bail, Result},
    askama::Template,
    heck::{ToKebabCase, ToSnakeCase, ToSnekCase, ToUpperCamelCase},
    std::fs::{self},
};

pub fn parse(path: String, output: String, as_crate: bool) -> Result<()> {
    let (accounts_data, instructions_data, types_data, events_data, program_name) =
        match read_idl(&path) {
            Ok(idl) => {
                let accounts_data = process_accounts(&idl);
                let instructions_data = process_instructions(&idl);
                let types_data = process_types(&idl);
                let events_data = process_events(&idl);
                let program_name = idl.metadata.name;

                (
                    accounts_data,
                    instructions_data,
                    types_data,
                    events_data,
                    program_name,
                )
            }
            Err(_legacy_idl_err) => match legacy_read_idl(&path) {
                Ok(idl) => {
                    let accounts_data = legacy_process_accounts(&idl);
                    let instructions_data = legacy_process_instructions(&idl);
                    let types_data = legacy_process_types(&idl);
                    let events_data = legacy_process_events(&idl);
                    let program_name = idl.name;

                    (
                        accounts_data,
                        instructions_data,
                        types_data,
                        events_data,
                        program_name,
                    )
                }
                Err(idl_err) => {
                    bail!("{idl_err}");
                }
            },
        };

    let decoder_name = format!("{}Decoder", program_name.to_upper_camel_case());
    let decoder_name_kebab = program_name.to_kebab_case();
    let program_struct_name = format!("{}Account", program_name.to_upper_camel_case());
    let program_instruction_enum = format!("{}Instruction", program_name.to_upper_camel_case());

    let crate_dir = if output.ends_with("/") {
        if as_crate {
            format!("{}{}-decoder", output, decoder_name_kebab)
        } else {
            format!("{}{}_decoder", output, program_name.to_snek_case())
        }
    } else if as_crate {
        format!("{}/{}-decoder", output, decoder_name_kebab)
    } else {
        format!("{}/{}_decoder", output, program_name.to_snek_case())
    };

    fs::create_dir_all(&crate_dir).expect("Failed to create decoder directory");

    let src_dir = if as_crate {
        format!("{}/src", crate_dir)
    } else {
        crate_dir.clone()
    };

    fs::create_dir_all(&src_dir).expect("Failed to create src directory");

    let needs_big_array = types_data.iter().any(|type_data| {
        type_data.fields.iter().any(|field| {
            field.rust_type.starts_with("[")
                && field.rust_type.ends_with("]")
                && is_big_array(&field.rust_type)
        })
    });

    // Generate types
    let types_dir = format!("{}/types", src_dir);
    fs::create_dir_all(&types_dir).expect("Failed to create types directory");

    for type_data in &types_data {
        let template = TypeStructTemplate { type_data };
        let rendered = template.render().unwrap();
        let filename = format!("{}/{}.rs", types_dir, type_data.name.to_snake_case());
        fs::write(&filename, rendered).expect("Failed to write type struct file");
        println!("Generated {}", filename);
    }

    let mut types_mod_content = types_data
        .iter()
        .map(|type_data| {
            format!(
                "pub mod {};\npub use {}::*;",
                type_data.name.to_snake_case(),
                type_data.name.to_snake_case()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    if needs_big_array {
        types_mod_content.push_str("\nuse serde_big_array::BigArray;\n");
    }

    let types_mod_filename = format!("{}/mod.rs", types_dir);
    fs::write(&types_mod_filename, types_mod_content).expect("Failed to write types mod file");
    println!("Generated {}", types_mod_filename);

    // Generate Accounts

    let accounts_dir = format!("{}/accounts", src_dir);
    fs::create_dir_all(&accounts_dir).expect("Failed to create accounts directory");

    for account in &accounts_data {
        let template = AccountsStructTemplate { account };
        let rendered = template.render().unwrap();
        let filename = format!("{}/{}.rs", accounts_dir, account.module_name);
        fs::write(&filename, rendered).expect("Failed to write account struct file");
        println!("Generated {}", filename);
    }

    let accounts_mod_template = AccountsModTemplate {
        accounts: &accounts_data,
        decoder_name: decoder_name.clone(),
        program_struct_name: program_struct_name.clone(),
    };
    let accounts_mod_rendered = accounts_mod_template.render().unwrap();
    let accounts_mod_filename = format!("{}/mod.rs", accounts_dir);

    fs::write(&accounts_mod_filename, accounts_mod_rendered)
        .expect("Failed to write accounts mod file");
    println!("Generated {}", accounts_mod_filename);

    // Generate Instructions

    let instructions_dir = format!("{}/instructions", src_dir);
    fs::create_dir_all(&instructions_dir).expect("Failed to create instructions directory");

    for instruction in &instructions_data {
        let template = InstructionsStructTemplate { instruction };
        let rendered = template.render().unwrap();
        let filename = format!("{}/{}.rs", instructions_dir, instruction.module_name);
        fs::write(&filename, rendered).expect("Failed to write instruction struct file");
        println!("Generated {}", filename);
    }

    for event in &events_data {
        let template = EventsStructTemplate { event };
        let rendered = template.render().unwrap();
        let filename = format!("{}/{}.rs", instructions_dir, event.module_name);
        fs::write(&filename, rendered).expect("Failed to write event struct file");
        println!("Generated {}", filename);
    }

    let instructions_mod_template = InstructionsModTemplate {
        instructions: &instructions_data,
        decoder_name: decoder_name.clone(),
        program_instruction_enum: program_instruction_enum.clone(),
        events: &events_data,
    };
    let instructions_mod_rendered = instructions_mod_template.render().unwrap();
    let instructions_mod_filename = format!("{}/mod.rs", instructions_dir);

    fs::write(&instructions_mod_filename, instructions_mod_rendered)
        .expect("Failed to write instructions mod file");

    println!("Generated {}", instructions_mod_filename);

    if as_crate {
        let lib_rs_content = format!(
            "pub struct {decoder_name};\npub mod accounts;\npub mod instructions;\npub mod types;",
            decoder_name = decoder_name
        );
        let lib_rs_filename = format!("{}/lib.rs", src_dir);
        fs::write(&lib_rs_filename, lib_rs_content).expect("Failed to write lib.rs file");
        println!("Generated {}", lib_rs_filename);

        let cargo_toml_content = format!(
            r#"[package]
name = "{decoder_name_kebab}-decoder"
version = "0.6.1"
edition = {{ workspace = true }}

[lib]
crate-type = ["rlib"]

[dependencies]
carbon-core = {{ workspace = true }}
carbon-proc-macros = {{ workspace = true }}
carbon-macros = {{ workspace = true }}
solana-sdk = {{ workspace = true }}
serde = {{ workspace = true }}
{big_array}
"#,
            decoder_name_kebab = decoder_name_kebab,
            big_array = if needs_big_array {
                "serde-big-array = { workspace = true }"
            } else {
                ""
            }
        );
        let cargo_toml_filename = format!("{}/Cargo.toml", crate_dir);
        fs::write(&cargo_toml_filename, cargo_toml_content)
            .expect("Failed to write Cargo.toml file");
        println!("Generated {}", cargo_toml_filename);
    } else {
        let mod_rs_content = format!(
            "pub struct {decoder_name};\npub mod accounts;\npub mod instructions;\npub mod types;",
            decoder_name = decoder_name
        );
        let mod_rs_filename = format!("{}/mod.rs", src_dir);
        fs::write(&mod_rs_filename, mod_rs_content).expect("Failed to write mod.rs file");
        println!("Generated {}", mod_rs_filename);
    }

    Ok(())
}

pub fn scaffold(
    output: String,
    decoders: String,
    data_source: String,
    metrics: String,
) -> Result<()> {
    Ok(())
}
