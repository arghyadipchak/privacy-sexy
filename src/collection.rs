use crate::OS;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_yaml::{from_reader, Value};
use std::{fs::File, path::Path};

/**
Wraps the `code_string` in comments and adds an echo call

# Examples

```no_run
assert_eq!(r#"
## ------------------------------------------------------------
## ---------------------Clear bash history---------------------
## ------------------------------------------------------------
echo --- Clear bash history
rm -f ~/.bash_history
## ------------------------------------------------------------
"#,
beautify("rm -f ~/.bash_history", "Clear bash history", &OS::Linux, false)
)
```
*/
fn beautify(code_string: &str, name: &str, os: &OS, revert: bool) -> String {
    let mut name = name.to_string();
    if revert {
        name.push_str(" (revert)");
    }

    if let OS::Windows = os {
        format!(
            ":: {0:-^60}\n:: {1:-^60}\n:: {0:-^60}\necho --- {1}\n{2}\n:: {0:-^60}",
            "", name, code_string
        )
    } else {
        format!(
            "# {0:-^60}\n# {1:-^60}\n# {0:-^60}\necho --- {1}\n{2}\n# {0:-^60}",
            "", name, code_string
        )
    }
}

/**
Applies pipe on `text`. Following pipes are available:
- escapeDoubleQuotes
- inlinePowerShell

# Panics

Panics for invalid regex expressions

# Examples

```no_run
assert_eq!("\"^\"\"Hello\"^\"\"", piper("escapeDoubleQuotes", "\"Hello\""));
```
*/
pub fn piper(pipe: &str, text: &str) -> String {
    match pipe {
        "escapeDoubleQuotes" => text.replace('\"', "\"^\"\""),
        "inlinePowerShell" => {
            // Inline comments
            let t = Regex::new(r"<#\s*(.*)#>|#\s*(.*)")
                .unwrap()
                .replace_all(text, |c: &Captures| {
                    c.get(1)
                        .map_or(String::new(), |m| format!("<# {} #>", m.as_str().trim()))
                });

            // Here strings
            let t = Regex::new(r#"@(['"])\s*(?:\r\n|\r|\n)((.|\n|\r)+?)(\r\n|\r|\n)['"]@"#)
                .unwrap()
                .replace_all(&t, |c: &Captures| {
                    let (quotes, escaped_quotes, separator) = match c.get(1).map_or("'", |m| m.as_str()) {
                        "'" => ("'", "''", "'+\"`r`n\"+'"),
                        _ => ("\"", "`\"", "`r`n"),
                    };

                    format!(
                        "{0}{1}{0}",
                        quotes,
                        Regex::new(r"\r\n|\r|\n")
                            .unwrap()
                            .split(&c.get(2).map_or("", |m| m.as_str()).replace(quotes, escaped_quotes))
                            .collect::<Vec<&str>>()
                            .join(separator)
                    )
                });

            // Merge lines with back tick
            let t = Regex::new(r" +`\s*(?:\r\n|\r|\n)\s*").unwrap().replace_all(&t, " ");

            // Merge lines
            Regex::new(r"\r\n|\r|\n")
                .unwrap()
                .split(&t)
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<&str>>()
                .join("; ")
        }
        _ => text.to_string(),
    }
}

// TODO
fn parse_start_end(code: &str) -> String {
    code.to_string()
}

/**
### `ParseError`
*/
#[derive(Debug)]
pub enum Error {
    FunctionNotFound(String),
    ParameterNotFound(String),
    CallCodeNotFound(String),
}

/**
### `Collection`

- A collection simply defines:
  - different categories and their scripts in a tree structure
  - OS specific details
- Also allows defining common [function](FunctionData)s to be used throughout the collection if
  you'd like different scripts to share same code.
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionData {
    /// - Operating system that the [Collection](CollectionData) is written for.
    /// - 📖 See [crate](OS) enum for allowed values.
    pub os: OS,
    /// - Defines the scripting language that the code of other action uses.
    pub scripting: ScriptingDefinitionData,
    /// - Each [category](CategoryData) is rendered as different cards in card presentation.
    /// - ❗ A [Collection](CollectionData) must consist of at least one category.
    pub actions: Vec<CategoryData>,
    /// - Functions are optionally defined to re-use the same code throughout different scripts.
    pub functions: Option<Vec<FunctionData>>,
}

impl CollectionData {
    /**
    Reads [`CollectionData`] from file at `path`

    # Errors

    Returns [`Err`] if:
    - file cannot be opened OR
    - contents of file cannot be deserialized
    into [`CollectionData`]
    */
    pub fn from_file(path: impl AsRef<Path>) -> Result<CollectionData, Box<dyn std::error::Error>> {
        Ok(from_reader::<File, CollectionData>(File::open(path)?)?)
    }

    /**
    Parses [`CollectionData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    pub fn parse(
        &self,
        names: Option<&Vec<String>>,
        revert: bool,
        recommend: Option<Recommend>,
    ) -> Result<String, Error> {
        Ok(format!(
            "{}\n\n\n{}\n\n\n{}",
            parse_start_end(&self.scripting.start_code),
            self.actions
                .iter()
                .map(|action| action.parse(names, &self.functions, &self.os, revert, recommend))
                .collect::<Result<Vec<String>, Error>>()?
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<String>>()
                .join("\n\n\n"),
            parse_start_end(&self.scripting.end_code),
        ))
    }
}

/**
### `Category`

- Category has a parent that has tree-like structure where it can have subcategories or subscripts.
- It's a logical grouping of different scripts and other categories.
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryData {
    /// - ❗ Category must consist of at least one subcategory or script.
    /// - Children can be combination of scripts and subcategories.
    pub children: Vec<CategoryOrScriptData>,
    /// - Name of the category
    /// - ❗ Must be unique throughout the [Collection](CollectionData)
    pub category: String,
    /// - Single documentation URL or list of URLs for those who wants to learn more about the script
    /// - E.g. `https://docs.microsoft.com/en-us/windows-server/`
    pub docs: Option<DocumentationUrlsData>,
}

impl CategoryData {
    /**
    Parses [`CategoryData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    pub fn parse(
        &self,
        names: Option<&Vec<String>>,
        funcs: &Option<Vec<FunctionData>>,
        os: &OS,
        revert: bool,
        recommend: Option<Recommend>,
    ) -> Result<String, Error> {
        let (names, recommend) = if names.map_or(false, |ns| ns.contains(&self.category)) {
            (None, None)
        } else {
            (names, recommend)
        };

        Ok(self
            .children
            .iter()
            .map(|child| child.parse(names, funcs, os, revert, recommend))
            .collect::<Result<Vec<String>, Error>>()?
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<String>>()
            .join("\n\n\n"))
    }
}

/// Enum to hold possible values
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CategoryOrScriptData {
    /// Refer to [Collection](CategoryData)
    CategoryData(CategoryData),
    /// Refer to [Collection](ScriptData)
    ScriptData(ScriptData),
}

impl CategoryOrScriptData {
    /**
    Parses [`CategoryOrScriptData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    fn parse(
        &self,
        names: Option<&Vec<String>>,
        funcs: &Option<Vec<FunctionData>>,
        os: &OS,
        revert: bool,
        recommend: Option<Recommend>,
    ) -> Result<String, Error> {
        match self {
            CategoryOrScriptData::CategoryData(data) => data.parse(names, funcs, os, revert, recommend),
            CategoryOrScriptData::ScriptData(data) => data.parse(names, funcs, os, revert, recommend),
        }
    }
}

/// - Single documentation URL or list of URLs for those who wants to learn more about the script
/// - E.g. `https://docs.microsoft.com/en-us/windows-server/`
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentationUrlsData {
    /// Multiple URLs
    VecStrings(Vec<String>),
    /// Single URL
    String(String),
}

/**
### `FunctionParameter`

- Defines a parameter that function requires optionally or mandatory.
- Its arguments are provided by a [Script](ScriptData) through a [FunctionCall](FunctionCallData).
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct ParameterDefinitionData {
    /**
    - Name of the parameters that the function has.
    - Parameter names must be defined to be used in [expressions (templating)](./README.md#expressions).
    - ❗ Parameter names must be unique and include alphanumeric characters only.
    */
    pub name: String,
    /**
    - Specifies whether the caller [Script](ScriptData) must provide any value for the parameter.
    - If set to `false` i.e. an argument value is not optional then it expects a non-empty value for the variable;
      - Otherwise it throws.
    - 💡 Set it to `true` if a parameter is used conditionally;
      - Or else set it to `false` for verbosity or do not define it as default value is `false` anyway.
    - 💡 Can be used in conjunction with [`with` expression](./README.md#with).
    */
    #[serde(default)]
    pub optional: bool,
}

/**
### `Function`

- Functions allow re-usable code throughout the defined scripts.
- Functions are templates compiled by privacy.sexy and uses special expression expressions.
- A function can be of two different types (just like [scripts](ScriptData)):
  1. Inline function: a function with an inline code.
     - Must define `code` property and optionally `revertCode` but not `call`.
  2. Caller function: a function that calls other functions.
     - Must define `call` property but not `code` or `revertCode`.
- 👀 Read more on [Templating](./README.md) for function expressions and [example usages](./README.md#parameter-substitution).
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionData {
    /**
    - Name of the function that scripts will use.
    - Convention is to use camelCase, and be verbs.
    - E.g. `uninstallStoreApp`
    - ❗ Function names must be unique
    */
    pub name: String,
    /**
    - Batch file commands that will be executed
    - 💡 [Expressions (templating)](./README.md#expressions) can be used in its value
    - 💡 If defined, best practice to also define `revertCode`
    - ❗ If not defined `call` must be defined
    */
    pub code: Option<String>,
    /**
    - Code that'll undo the change done by `code` property.
    - E.g. let's say `code` sets an environment variable as `setx POWERSHELL_TELEMETRY_OPTOUT 1`
      - then `revertCode` should be doing `setx POWERSHELL_TELEMETRY_OPTOUT 0`
    - 💡 [Expressions (templating)](./README.md#expressions) can be used in code
    */
    #[serde(rename = "revertCode")]
    pub revert_code: Option<String>,
    /**
    - A shared function or sequence of functions to call (called in order)
    - The parameter values that are sent can use [expressions (templating)](./README.md#expressions)
    - ❗ If not defined `code` must be defined
    */
    pub call: Option<FunctionCallsData>,
    /**
    - List of parameters that function code refers to.
    - ❗ Must be defined to be able use in [`FunctionCall`](FunctionCallData) or [expressions (templating)](./README.md#expressions)
    `code`: *`string`* (**required** if `call` is undefined)
    - Batch file commands that will be executed
    - 💡 [Expressions (templating)](./README.md#expressions) can be used in its value
    - 💡 If defined, best practice to also define `revertCode`
    - ❗ If not defined `call` must be defined
    */
    pub parameters: Option<Vec<ParameterDefinitionData>>,
}

impl FunctionData {
    /**
    Parses [`FunctionData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    fn parse(
        &self,
        params: &Option<FunctionCallParametersData>,
        funcs: &Option<Vec<FunctionData>>,
        os: &OS,
        revert: bool,
    ) -> Result<String, Error> {
        let mut parsed = {
            if let Some(fcd) = &self.call {
                fcd.parse(funcs, os, revert)?
            } else if let Some(code_string) = if revert { &self.revert_code } else { &self.code } {
                code_string.to_string()
            } else {
                return Err(Error::CallCodeNotFound(self.name.clone()));
            }
        };

        if let Some(vec_pdd) = &self.parameters {
            for pdd in vec_pdd {
                parsed = match params.as_ref().and_then(|p| p.get(&pdd.name)) {
                    Some(v) => {
                        if pdd.optional {
                            parsed = Regex::new(&format!(
                                r"(?s)\{{\{{\s*with\s*\${}\s*\}}\}}\s?(.*?)\s?\{{\{{\s*end\s*\}}\}}",
                                &pdd.name
                            ))
                            .unwrap()
                            .replace_all(&parsed, |c: &Captures| {
                                c.get(1)
                                    .map_or("", |m| m.as_str())
                                    .replace("{{ . ", &format!("{{{{ ${} ", &pdd.name))
                            })
                            .to_string();
                        }

                        Regex::new(format!(r"\{{\{{\s*\${}\s*((\|\s*\w*\s*)*)\}}\}}", &pdd.name).as_str())
                            .unwrap()
                            .replace_all(&parsed, |c: &Captures| {
                                c.get(1)
                                    .map_or("", |m| m.as_str())
                                    .split('|')
                                    .map(str::trim)
                                    .filter(|p| !p.is_empty())
                                    .fold(v.as_str().unwrap().to_string(), |v, pipe| piper(pipe.trim(), &v))
                            })
                    }
                    None => {
                        if pdd.optional {
                            Regex::new(&format!(
                                r"(?s)\{{\{{\s*with\s*\${}\s*\}}\}}\s?(.*?)\s?\{{\{{\s*end\s*\}}\}}",
                                &pdd.name
                            ))
                            .unwrap()
                            .replace_all(&parsed, "")
                        } else {
                            return Err(Error::ParameterNotFound(pdd.name.clone()));
                        }
                    }
                }
                .to_string();
            }
        }

        Ok(parsed)
    }
}

/**
- Defines key value dictionary for each parameter and its value
- E.g.

  ```yaml
    parameters:
      userDefinedParameterName: parameterValue
      # ...
      appName: Microsoft.WindowsFeedbackHub
  ```

- 💡 [Expressions (templating)](./README.md#expressions) can be used as parameter value
*/
pub type FunctionCallParametersData = Value;

/**
### `FunctionCall`

- Describes a single call to a function by optionally providing values to its parameters.
- 👀 See [parameter substitution](./README.md#parameter-substitution) for an example usage
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCallData {
    /// - Name of the function to call.
    /// - ❗ Function with same name must defined in `functions` property of [Collection](CollectionData)
    pub function: String,
    /**
    - Defines key value dictionary for each parameter and its value
    - E.g.

      ```yaml
        parameters:
          userDefinedParameterName: parameterValue
          # ...
          appName: Microsoft.WindowsFeedbackHub
      ```

    - 💡 [Expressions (templating)](./README.md#expressions) can be used as parameter value
    */
    pub parameters: Option<FunctionCallParametersData>,
}

impl FunctionCallData {
    /**
    Parses [`FunctionCallData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    fn parse(&self, funcs: &Option<Vec<FunctionData>>, os: &OS, revert: bool) -> Result<String, Error> {
        funcs
            .as_ref()
            .and_then(|vec_fd| vec_fd.iter().find(|fd| fd.name == self.function))
            .map_or(Err(Error::FunctionNotFound(self.function.clone())), |fd| {
                fd.parse(&self.parameters, funcs, os, revert)
            })
    }
}

/// Possible parameters of a function call i.e. either one parameter or multiple parameters
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallsData {
    /// Multiple Parameter
    VecFunctionCallData(Vec<FunctionCallData>),
    /// Single Parameter
    FunctionCallData(FunctionCallData),
}

impl FunctionCallsData {
    /**
    Parses [`FunctionCallsData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    fn parse(&self, funcs: &Option<Vec<FunctionData>>, os: &OS, revert: bool) -> Result<String, Error> {
        match &self {
            FunctionCallsData::VecFunctionCallData(vec_fcd) => Ok(vec_fcd
                .iter()
                .map(|fcd| fcd.parse(funcs, os, revert))
                .collect::<Result<Vec<String>, Error>>()?
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<String>>()
                .join("\n\n")),
            FunctionCallsData::FunctionCallData(fcd) => fcd.parse(funcs, os, revert),
        }
    }
}

/**
### `Script`

- Script represents a single tweak.
- A script can be of two different types (just like [functions](FunctionData)):
  1. Inline script; a script with an inline code
     - Must define `code` property and optionally `revertCode` but not `call`
  2. Caller script; a script that calls other functions
     - Must define `call` property but not `code` or `revertCode`
- 🙏 For any new script, please add `revertCode` and `docs` values if possible.
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptData {
    /// - Name of the script
    /// - ❗ Must be unique throughout the [Collection](CollectionData)
    pub name: String,
    /**
    - Batch file commands that will be executed
    - 💡 If defined, best practice to also define `revertCode`
    - ❗ If not defined `call` must be defined, do not define if `call` is defined.
    */
    pub code: Option<String>,
    /**
    - Code that'll undo the change done by `code` property.
    - E.g. let's say `code` sets an environment variable as `setx POWERSHELL_TELEMETRY_OPTOUT 1`
      - then `revertCode` should be doing `setx POWERSHELL_TELEMETRY_OPTOUT 0`
    - ❗ Do not define if `call` is defined.
    */
    #[serde(rename = "revertCode")]
    pub revert_code: Option<String>,
    /// - A shared function or sequence of functions to call (called in order)
    /// - ❗ If not defined `code` must be defined
    pub call: Option<FunctionCallsData>,
    /// - Single documentation URL or list of URLs for those who wants to learn more about the script
    /// - E.g. `https://docs.microsoft.com/en-us/windows-server/`
    pub docs: Option<DocumentationUrlsData>,
    /**
    - If not defined then the script will not be recommended
    - If defined it can be either
      - `standard`: Only non-breaking scripts without limiting OS functionality
      - `strict`: Scripts that can break certain functionality in favor of privacy and security
    */
    pub recommend: Option<Recommend>,
}

impl ScriptData {
    /**
    Parses [`ScriptData`] into String

    # Errors

    Returns [`Error`] if the object is not parsable
    */
    pub fn parse(
        &self,
        names: Option<&Vec<String>>,
        funcs: &Option<Vec<FunctionData>>,
        os: &OS,
        revert: bool,
        recommend: Option<Recommend>,
    ) -> Result<String, Error> {
        if (recommend.is_some() && recommend > self.recommend) || names.map_or(false, |n| !n.contains(&self.name)) {
            Ok(String::new())
        } else if let Some(fcd) = &self.call {
            Ok(beautify(&fcd.parse(funcs, os, revert)?, &self.name, os, revert))
        } else if let Some(code_string) = if revert { &self.revert_code } else { &self.code } {
            Ok(beautify(code_string, &self.name, os, revert))
        } else {
            Err(Error::CallCodeNotFound(self.name.clone()))
        }
    }
}

/**
### `ScriptingDefinition`

- Defines global properties for scripting that's used throughout its parent [Collection](CollectionData).
*/
#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptingDefinitionData {
    /// Name of the Script
    pub language: String,
    /// Optional file extension for the said script
    #[serde(rename = "fileExtension")]
    pub file_extension: Option<String>,
    /**
    - Code that'll be inserted on top of user created script.
    - Global variables such as `$homepage`, `$version`, `$date` can be used using
      [parameter substitution](./README.md#parameter-substitution) code syntax such as `Welcome to {{ $homepage }}!`
    */
    #[serde(rename = "startCode")]
    pub start_code: String,
    /**
    - Code that'll be inserted at the end of user created script.
    - Global variables such as `$homepage`, `$version`, `$date` can be used using
      [parameter substitution](./README.md#parameter-substitution) code syntax such as `Welcome to {{ $homepage }}!
    */
    #[serde(rename = "endCode")]
    pub end_code: String,
}

/**
- If not defined then the script will not be recommended
- If defined it can be either
  - `standard`: Only non-breaking scripts without limiting OS functionality
  - `strict`: Scripts that can break certain functionality in favor of privacy and security
*/
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum Recommend {
    /// - `strict`: Scripts that can break certain functionality in favor of privacy and security
    #[serde(rename = "strict")]
    Strict,
    /// - `standard`: Only non-breaking scripts without limiting OS functionality
    #[serde(rename = "standard")]
    Standard,
}
