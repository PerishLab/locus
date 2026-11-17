#[derive(Default)]
enum Quote {
    #[default]
    Bare,
    Single,
    Double,
}

#[derive(Default)]
struct Scan {
    quote: Quote,
    escaped: bool,
}

pub(super) fn words(command: &str) -> Result<Vec<String>, &'static str> {
    let mut scan = Scan::default();
    for character in command.chars() {
        scan.take(character)?;
    }
    shlex::split(command).ok_or("shell-unparsed")
}

impl Scan {
    fn take(&mut self, character: char) -> Result<(), &'static str> {
        if character.is_control() && character != '\t' {
            return Err("shell-structure");
        }
        if self.escaped {
            self.escaped = false;
            return Ok(());
        }
        match self.quote {
            Quote::Single => self.single(character),
            Quote::Double => self.double(character)?,
            Quote::Bare => self.bare(character)?,
        }
        Ok(())
    }

    fn single(&mut self, character: char) {
        if character == '\'' {
            self.quote = Quote::Bare;
        }
    }

    fn double(&mut self, character: char) -> Result<(), &'static str> {
        match character {
            '"' => self.quote = Quote::Bare,
            '\\' => self.escaped = true,
            '$' | '`' => return Err("shell-expansion"),
            _ => {}
        }
        Ok(())
    }

    fn bare(&mut self, character: char) -> Result<(), &'static str> {
        match character {
            '\'' => self.quote = Quote::Single,
            '"' => self.quote = Quote::Double,
            '\\' => self.escaped = true,
            '$' | '`' | '*' | '?' | '[' | ']' | '{' | '}' | '~' | '!' => {
                return Err("shell-expansion");
            }
            ';' | '&' | '|' | '(' | ')' | '<' | '>' | '#' => {
                return Err("shell-structure");
            }
            _ => {}
        }
        Ok(())
    }
}
