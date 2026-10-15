use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "--call-frames") {
        if arguments.len() != 11 {
            eprintln!("usage: semantic-route-ast-extractor --call-frames SCHEMA INDEXED EXPLICIT SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..6].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit frame dispatch source",
            "slot codec source",
            "slot freeze helper source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_call_frame_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3],
            &sources[4],
            &arguments[6],
            &arguments[7],
            &arguments[8],
            &arguments[9],
            &arguments[10],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("call frame inventory: {error}");
                ExitCode::from(2)
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "--field-index") {
        if arguments.len() != 11 {
            eprintln!("usage: semantic-route-ast-extractor --field-index SCHEMA INDEXED EXPLICIT SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..6].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit frame dispatch source",
            "slot codec source",
            "slot freeze helper source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_field_index_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3],
            &sources[4],
            &arguments[6],
            &arguments[7],
            &arguments[8],
            &arguments[9],
            &arguments[10],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("field/index inventory: {error}");
                ExitCode::from(2)
            }
        };
    }
    if arguments.first().is_some_and(|argument| argument == "--result-constructors") {
        if arguments.len() != 7 {
            eprintln!("usage: semantic-route-ast-extractor --result-constructors SCHEMA INDEXED EXPLICIT SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..4].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit dispatch source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_result_constructor_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &arguments[4],
            &arguments[5],
            &arguments[6],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("result constructor inventory: {error}");
                ExitCode::from(2)
            }
        };
    }

    if arguments.first().is_some_and(|argument| argument == "--collection-constructors") {
        if arguments.len() != 9 {
            eprintln!("usage: semantic-route-ast-extractor --collection-constructors SCHEMA INDEXED EXPLICIT OPERANDS SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH OPERANDS_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..5].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit frame dispatch source",
            "indexed operand helper source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_collection_construction_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3],
            &arguments[5],
            &arguments[6],
            &arguments[7],
            &arguments[8],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("collection construction inventory: {error}");
                ExitCode::from(2)
            }
        };
    }

    if arguments.first().is_some_and(|argument| argument == "--frame-constructors") {
        if arguments.len() != 7 {
            eprintln!("usage: semantic-route-ast-extractor --frame-constructors SCHEMA INDEXED EXPLICIT SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..4].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit dispatch source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_frame_constructor_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &arguments[4],
            &arguments[5],
            &arguments[6],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("frame constructor inventory: {error}");
                ExitCode::from(2)
            }
        };
    }

    if arguments.first().is_some_and(|argument| argument == "--eager-atoms") {
        if arguments.len() != 11 {
            eprintln!("usage: semantic-route-ast-extractor --eager-atoms SCHEMA INDEXED EXPLICIT SLOT_CODEC FREEZE_HELPER SCHEMA_PATH INDEXED_PATH EXPLICIT_PATH SLOT_CODEC_PATH FREEZE_HELPER_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..6].iter().zip([
            "schema source",
            "indexed dispatch source",
            "explicit dispatch source",
            "slot codec source",
            "slot freeze helper source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_eager_atom_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3],
            &sources[4],
            &arguments[6],
            &arguments[7],
            &arguments[8],
            &arguments[9],
            &arguments[10],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("eager atom inventory: {error}");
                ExitCode::from(2)
            }
        };
    }

    if arguments.first().is_some_and(|argument| argument == "--typed-scalars") {
        if arguments.len() != 9 {
            eprintln!("usage: semantic-route-ast-extractor --typed-scalars SCHEMA INDEXED VALUES EXPLICIT SCHEMA_PATH INDEXED_PATH VALUES_PATH EXPLICIT_PATH");
            return ExitCode::from(2);
        }
        let mut sources = Vec::new();
        for (path, label) in arguments[1..5].iter().zip([
            "schema source",
            "indexed dispatch source",
            "typed values source",
            "explicit dispatch source",
        ]) {
            match fs::read_to_string(path) {
                Ok(source) => sources.push(source),
                Err(error) => {
                    eprintln!("read {label}: {error}");
                    return ExitCode::from(2);
                }
            }
        }
        return match semantic_route_ast_extractor::extract_typed_scalar_inventory(
            &sources[0],
            &sources[1],
            &sources[2],
            &sources[3],
            &arguments[5],
            &arguments[6],
            &arguments[7],
            &arguments[8],
        ) {
            Ok(inventory) => {
                println!("{}", inventory.to_json());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("typed scalar inventory: {error}");
                ExitCode::from(2)
            }
        };
    }

    if arguments.len() != 3 {
        eprintln!("usage: semantic-route-ast-extractor SCHEMA DISPATCH DISPATCH_PATH");
        return ExitCode::from(2);
    }
    let schema = match fs::read_to_string(&arguments[0]) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("read schema source: {error}");
            return ExitCode::from(2);
        }
    };
    let dispatch = match fs::read_to_string(&arguments[1]) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("read dispatch source: {error}");
            return ExitCode::from(2);
        }
    };
    match semantic_route_ast_extractor::extract_statement_inventory(
        &schema,
        &dispatch,
        &arguments[2],
    ) {
        Ok(inventory) => {
            println!("{}", inventory.to_json());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("statement inventory: {error}");
            ExitCode::from(2)
        }
    }
}
