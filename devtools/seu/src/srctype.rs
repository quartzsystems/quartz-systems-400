// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Source types: how each one is edited (first column shown, upper
//! case), checked (the analyzer, the DDS compilers, a hook for RPG) and
//! prompted, and the format lines and prompt layouts of the fixed-form
//! specifications, as column tables. The tables are the compatibility
//! interface: the columns are IBM's; the ruler words name the columns.

/// The language a source type belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// CL (`CLP`, `CLLE`, `CL`).
    Cl,
    /// Command definitions (`CMD`).
    Cmd,
    /// DDS (`PF`, `LF`, `DSPF`, `PRTF`, `ICFF`).
    Dds,
    /// RPG IV (`RPGLE`, `SQLRPGLE`).
    RpgIle,
    /// RPG III (`RPG`, `SQLRPG`, `RPG38`).
    Rpg,
    /// Anything else: free-form text.
    Other,
}

/// How a source type is edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceType {
    /// The type's name, upper case; blank for none.
    pub name: String,
    /// Its language.
    pub family: Family,
}

/// Which syntax checker a type has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checker {
    /// The command analyzer, one command at a time.
    Cl,
    /// The DDS compiler of the file type.
    Dds,
    /// RPG: a hook with no checker behind it yet.
    Rpg,
    /// None.
    None,
}

impl SourceType {
    /// The type of a member (blank for none).
    #[must_use]
    pub fn new(name: &str) -> Self {
        let name = name.trim().to_ascii_uppercase();
        let family = match name.as_str() {
            "CL" | "CLP" | "CLLE" | "CLP38" | "CL38" => Family::Cl,
            "CMD" | "CMD38" => Family::Cmd,
            "PF" | "LF" | "DSPF" | "PRTF" | "ICFF" | "BSCF" | "MXDF" | "PF38" | "LF38"
            | "DSPF38" | "PRTF38" => Family::Dds,
            "RPGLE" | "SQLRPGLE" => Family::RpgIle,
            "RPG" | "SQLRPG" | "RPG38" | "RPT" | "RPT38" => Family::Rpg,
            _ => Family::Other,
        };
        Self { name, family }
    }

    /// Whether the type is one SEU knows (an unknown type follows the
    /// session default for upper case).
    #[must_use]
    pub fn known(&self) -> bool {
        self.family != Family::Other
            || matches!(self.name.as_str(), "TXT" | "C" | "CPP" | "H" | "CBL" | "CBLLE" | "SQL")
    }

    /// The first column the display shows.
    #[must_use]
    pub fn first_column(&self) -> usize {
        match self.family {
            Family::RpgIle | Family::Rpg => 6,
            _ => 1,
        }
    }

    /// Whether input is forced to upper case; `None` when the session
    /// default decides.
    #[must_use]
    pub fn upper(&self) -> Option<bool> {
        match self.family {
            Family::Cl | Family::Cmd | Family::Dds | Family::Rpg => Some(true),
            Family::RpgIle => Some(false),
            Family::Other if self.known() => Some(false),
            Family::Other => None,
        }
    }

    /// The syntax checker.
    #[must_use]
    pub fn checker(&self) -> Checker {
        match self.family {
            Family::Cl | Family::Cmd => Checker::Cl,
            Family::Dds => Checker::Dds,
            Family::Rpg | Family::RpgIle => Checker::Rpg,
            Family::Other => Checker::None,
        }
    }

    /// The prompt format of a record: by the specification letter in
    /// column 6 for RPG and DDS, a comment as `*`, else the free format.
    #[must_use]
    pub fn format_of(&self, data: &str) -> &'static str {
        let ch = |n: usize| data.chars().nth(n - 1).unwrap_or(' ').to_ascii_uppercase();
        match self.family {
            Family::RpgIle | Family::Rpg => {
                if ch(7) == '*' {
                    return "*";
                }
                match ch(6) {
                    'H' => "H",
                    'F' => "F",
                    'D' => "D",
                    'I' => "I",
                    'C' => {
                        let op: String = data.chars().skip(25).take(10).collect();
                        let factor1: String = data.chars().skip(11).take(14).collect();
                        if factor1.trim().is_empty() && op.trim().is_empty() || data.len() <= 35 {
                            "C"
                        } else if is_free_op(&op) {
                            "CX"
                        } else {
                            "C"
                        }
                    }
                    'O' => {
                        if data.chars().skip(6).take(10).all(|c| c == ' ') {
                            "P"
                        } else {
                            "O"
                        }
                    }
                    _ => "**",
                }
            }
            Family::Dds => {
                if ch(7) == '*' {
                    return "A*";
                }
                match self.name.as_str() {
                    "PF" | "PF38" => "PF",
                    "LF" | "LF38" => "LF",
                    "ICFF" | "BSCF" | "MXDF" => "BC",
                    _ => "DP",
                }
            }
            _ => "**",
        }
    }

    /// The prompt formats `Select Prompt` (`F23`) offers.
    #[must_use]
    pub fn prompt_formats(&self) -> &'static [&'static str] {
        match self.family {
            Family::RpgIle => &["H", "F", "D", "I", "C", "CX", "O", "P", "*", "**"],
            Family::Rpg => &["H", "F", "I", "C", "O", "P", "*", "**"],
            Family::Dds => &["PF", "LF", "DP", "BC", "A*", "**"],
            _ => &["**"],
        }
    }
}

/// Select Prompt's list for a type: a line per language, the prompt
/// formats it offers and what they are for.
#[must_use]
pub fn select_lines(st: &SourceType) -> Vec<String> {
    let mut out = Vec::new();
    let mut group = |name: &str, lines: &[&str]| {
        for (i, l) in lines.iter().enumerate() {
            let head = if i == 0 { name } else { "" };
            out.push(format!("  {head:<16}{l}"));
        }
    };
    match st.family {
        Family::RpgIle => group("ILE RPG:", &["H F D I C CX O P: the specifications;", "* comment, ** free format"]),
        Family::Rpg => group("RPG:", &["H F I C O P: the specifications;", "* comment, ** free format"]),
        Family::Dds => group("DDS:", &["PF physical, LF logical, DP display and printer,", "BC communications files; A* comment"]),
        _ => {}
    }
    group("Any type:", &["** free format"]);
    out
}

/// Operations whose factor 2 is extended (the `CX` prompt).
fn is_free_op(op: &str) -> bool {
    let o = op.trim().to_ascii_uppercase();
    let base = o.split('(').next().unwrap_or("");
    matches!(
        base,
        "EVAL" | "EVALR" | "IF" | "ELSEIF" | "DOW" | "DOU" | "WHEN" | "FOR" | "CALLP" | "RETURN"
            | "ON-ERROR" | "SORTA" | "XML-INTO" | "XML-SAX" | "DATA-INTO" | "MONITOR"
    )
}

/// A labelled column range of a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Col {
    /// First source column, 1-based.
    pub from: usize,
    /// Last source column.
    pub to: usize,
    /// The ruler word over the columns.
    pub ruler: &'static str,
}

const fn c(from: usize, to: usize, ruler: &'static str) -> Col {
    Col { from, to, ruler }
}

#[allow(clippy::too_many_lines)] // one table
/// The ruler columns of a format: the specification letter, then the
/// fields. `None` for the free format and comments.
fn columns(fmt: &str) -> Option<&'static [Col]> {
    Some(match fmt {
        "DP" | "BC" => {
            const T: &[Col] = &[
                c(6, 6, "A"),
                c(7, 7, "A"),
                c(8, 16, "N01N02N03"),
                c(17, 17, "T"),
                c(19, 28, "Name"),
                c(29, 29, "R"),
                c(30, 34, "Len"),
                c(35, 35, "T"),
                c(36, 37, "Dp"),
                c(38, 38, "B"),
                c(39, 41, "Lin"),
                c(42, 44, "Pos"),
                c(45, 80, "Functions"),
            ];
            T
        }
        "PF" | "LF" => {
            const T: &[Col] = &[
                c(6, 6, "A"),
                c(7, 7, "A"),
                c(8, 16, "N01N02N03"),
                c(17, 17, "T"),
                c(19, 28, "Name"),
                c(29, 29, "R"),
                c(30, 34, "Len"),
                c(35, 35, "T"),
                c(36, 37, "Dp"),
                c(38, 38, "B"),
                c(45, 80, "Functions"),
            ];
            T
        }
        "H" => {
            const T: &[Col] = &[c(6, 6, "H"), c(7, 80, "Keywords"), c(81, 100, "Comments")];
            T
        }
        "F" => {
            const T: &[Col] = &[
                c(6, 6, "F"),
                c(7, 16, "Filename"),
                c(17, 17, "I"),
                c(18, 18, "P"),
                c(19, 19, "E"),
                c(20, 20, "A"),
                c(21, 21, "S"),
                c(22, 22, "F"),
                c(23, 27, "Rlen"),
                c(28, 28, "L"),
                c(29, 33, "Klen"),
                c(34, 34, "A"),
                c(35, 35, "I"),
                c(36, 42, "Device"),
                c(44, 80, "Keywords"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "D" => {
            const T: &[Col] = &[
                c(6, 6, "D"),
                c(7, 21, "Name"),
                c(22, 22, "E"),
                c(23, 23, "T"),
                c(24, 25, "Ds"),
                c(26, 32, "From"),
                c(33, 39, "To/L"),
                c(40, 40, "I"),
                c(41, 42, "Dc"),
                c(44, 80, "Keywords"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "I" => {
            const T: &[Col] = &[
                c(6, 6, "I"),
                c(7, 16, "Filename"),
                c(17, 18, "Sq"),
                c(19, 19, "N"),
                c(20, 20, "O"),
                c(21, 22, "Id"),
                c(23, 27, "Pos1"),
                c(28, 28, "N"),
                c(29, 29, "C"),
                c(30, 30, "C"),
                c(31, 35, "Pos2"),
                c(36, 36, "N"),
                c(37, 37, "C"),
                c(38, 38, "C"),
                c(39, 43, "Pos3"),
                c(44, 44, "N"),
                c(45, 45, "C"),
                c(46, 46, "C"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "C" => {
            const T: &[Col] = &[
                c(6, 6, "C"),
                c(7, 8, "L0"),
                c(9, 11, "N01"),
                c(12, 25, "Factor1"),
                c(26, 35, "Opcode(E)"),
                c(36, 49, "Factor2"),
                c(50, 63, "Result"),
                c(64, 68, "Len"),
                c(69, 70, "D"),
                c(71, 72, "Hi"),
                c(73, 74, "Lo"),
                c(75, 76, "Eq"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "CX" => {
            const T: &[Col] = &[
                c(6, 6, "C"),
                c(7, 8, "L0"),
                c(9, 11, "N01"),
                c(12, 25, "Factor1"),
                c(26, 35, "Opcode(E)"),
                c(36, 80, "Extended-factor2"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "O" => {
            const T: &[Col] = &[
                c(6, 6, "O"),
                c(7, 16, "Filename"),
                c(17, 17, "D"),
                c(18, 20, "Add"),
                c(21, 29, "N01N02N03"),
                c(30, 39, "Excnam"),
                c(40, 42, "B"),
                c(43, 45, "A"),
                c(46, 48, "Sb"),
                c(49, 51, "Sa"),
                c(81, 100, "Comments"),
            ];
            T
        }
        "P" => {
            const T: &[Col] = &[
                c(6, 6, "O"),
                c(21, 29, "N01N02N03"),
                c(30, 43, "Field"),
                c(44, 44, "Y"),
                c(45, 45, "B"),
                c(47, 51, "End"),
                c(52, 52, "P"),
                c(53, 80, "Constant/editword"),
                c(81, 100, "Comments"),
            ];
            T
        }
        _ => return None,
    })
}

/// The numbered ruler character of a column: the tens digit on tens,
/// `+` on fives, blanks either side of the digit, dots elsewhere.
#[must_use]
pub fn ruler_char(col: usize) -> char {
    match col % 10 {
        0 => char::from_digit(u32::try_from(col / 10 % 10).unwrap_or(0), 10).unwrap_or('0'),
        1 | 9 => ' ',
        5 => '+',
        _ => '.',
    }
}

/// The format line of a format over columns `from..=to`: the label
/// (`FMT xx`, six wide) and the ruler.
#[must_use]
pub fn format_line(fmt: &str, from: usize, to: usize) -> (String, String) {
    let label = format!("FMT {fmt:<2}");
    let ruler: String = (from..=to).map(|col| ruler_at(fmt, col)).collect();
    (label, ruler)
}

/// The ruler character of a format at a column.
fn ruler_at(fmt: &str, col: usize) -> char {
    if fmt == "*" || fmt == "A*" {
        return match col {
            1..=5 | 8 => '.',
            6 => ' ',
            7 => '*',
            _ => ruler_char(col),
        };
    }
    let Some(cols) = columns(fmt) else {
        return ruler_char(col);
    };
    if col <= 5 {
        return '.';
    }
    for f in cols {
        if (f.from..=f.to).contains(&col) {
            let k = col - f.from;
            return f.ruler.chars().nth(k).unwrap_or('+');
        }
    }
    '.'
}

/// A field of a prompt: where it is relative to the `Prompt type`
/// row, and the source columns it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PField {
    /// Row offset from the `Prompt type` row.
    pub row: usize,
    /// Screen column of its first character.
    pub col: usize,
    /// First source column.
    pub from: usize,
    /// Last source column.
    pub to: usize,
}

/// A prompt layout: its first row, its labels, its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The `Prompt type` row on a 24-row display.
    pub top: usize,
    /// Labels: row offset, column, text.
    pub labels: &'static [(usize, usize, &'static str)],
    /// Fields.
    pub fields: &'static [PField],
}

const fn f(row: usize, col: usize, from: usize, to: usize) -> PField {
    PField { row, col, from, to }
}

/// The free-format prompt: the data area with a ruler.
pub const FREE: Layout = Layout {
    top: 14,
    labels: &[(2, 2, "Data area")],
    fields: {
        const T: &[PField] = &[f(4, 2, 1, 100)];
        T
    },
};

#[allow(clippy::too_many_lines)] // one table
/// The prompt layout of a format; the free format for any other.
#[must_use]
pub fn layout(fmt: &str) -> Layout {
    match fmt {
        "DP" | "BC" | "PF" | "LF" => Layout {
            top: 11,
            labels: &[
                (2, 2, "And/Or/"),
                (2, 29, "Name"),
                (3, 2, "Comment"),
                (3, 14, "N01N02N03"),
                (3, 29, "Type"),
                (3, 41, "Name"),
                (3, 54, "Ref"),
                (3, 63, "Length"),
                (5, 2, "Data"),
                (5, 14, "Decimal"),
                (5, 38, "--- Location ---"),
                (6, 2, "Type"),
                (6, 13, "Positions"),
                (6, 28, "Use"),
                (6, 38, "Line"),
                (6, 46, "Position"),
                (8, 2, "Functions"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(4, 5, 7, 7),
                    f(4, 14, 8, 16),
                    f(4, 30, 17, 17),
                    f(4, 38, 19, 28),
                    f(4, 55, 29, 29),
                    f(4, 63, 30, 34),
                    f(7, 3, 35, 35),
                    f(7, 16, 36, 37),
                    f(7, 29, 38, 38),
                    f(7, 38, 39, 41),
                    f(7, 48, 42, 44),
                    f(9, 2, 45, 80),
                ];
                T
            },
        },
        "H" => Layout {
            top: 15,
            labels: &[(2, 7, "Keywords"), (4, 7, "Comment")],
            fields: {
                const T: &[PField] = &[f(3, 7, 7, 80), f(5, 7, 81, 100)];
                T
            },
        },
        "F" => Layout {
            top: 7,
            labels: &[
                (2, 16, "File"),
                (2, 28, "File"),
                (2, 42, "End of"),
                (2, 56, "File"),
                (3, 2, "Filename"),
                (3, 16, "Type"),
                (3, 25, "Designation"),
                (3, 43, "File"),
                (3, 54, "Addition"),
                (3, 69, "Sequence"),
                (5, 2, "File"),
                (5, 12, "Record"),
                (5, 24, "Limits"),
                (5, 38, "Length of"),
                (5, 57, "Record"),
                (6, 2, "Format"),
                (6, 12, "Length"),
                (6, 22, "Processing"),
                (6, 38, "Key Field"),
                (6, 54, "Address Type"),
                (8, 6, "File"),
                (9, 2, "Organization"),
                (9, 22, "Device"),
                (9, 33, "Keywords"),
                (11, 2, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(4, 2, 7, 16),
                    f(4, 17, 17, 17),
                    f(4, 29, 18, 18),
                    f(4, 44, 19, 19),
                    f(4, 57, 20, 20),
                    f(4, 72, 21, 21),
                    f(7, 4, 22, 22),
                    f(7, 12, 23, 27),
                    f(7, 26, 28, 28),
                    f(7, 40, 29, 33),
                    f(7, 59, 34, 34),
                    f(10, 7, 35, 35),
                    f(10, 22, 36, 42),
                    f(10, 33, 44, 80),
                    f(12, 2, 81, 100),
                ];
                T
            },
        },
        "D" => Layout {
            top: 11,
            labels: &[
                (2, 30, "Declaration"),
                (2, 63, "To /"),
                (3, 2, "Name"),
                (3, 20, "E"),
                (3, 24, "S/U"),
                (3, 33, "Type"),
                (3, 51, "From"),
                (3, 62, "Length"),
                (5, 2, "Internal"),
                (5, 25, "Decimal"),
                (6, 2, "Data Type"),
                (6, 24, "Positions"),
                (6, 39, "Keywords"),
                (8, 2, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(4, 2, 7, 21),
                    f(4, 20, 22, 22),
                    f(4, 25, 23, 23),
                    f(4, 34, 24, 25),
                    f(4, 50, 26, 32),
                    f(4, 62, 33, 39),
                    f(7, 6, 40, 40),
                    f(7, 27, 41, 42),
                    f(7, 39, 44, 80),
                    f(9, 2, 81, 100),
                ];
                T
            },
        },
        "C" => Layout {
            top: 14,
            labels: &[
                (2, 2, "Level"),
                (2, 11, "N01"),
                (2, 15, "Factor 1"),
                (2, 32, "Operation"),
                (2, 50, "Factor 2"),
                (2, 65, "Result"),
                (4, 14, "Decimal"),
                (5, 2, "Length"),
                (5, 13, "Positions"),
                (5, 27, "HI"),
                (5, 32, "LO"),
                (5, 37, "EQ"),
                (5, 43, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(3, 3, 7, 8),
                    f(3, 11, 9, 11),
                    f(3, 15, 12, 25),
                    f(3, 32, 26, 35),
                    f(3, 50, 36, 49),
                    f(3, 65, 50, 63),
                    f(6, 2, 64, 68),
                    f(6, 16, 69, 70),
                    f(6, 27, 71, 72),
                    f(6, 32, 73, 74),
                    f(6, 37, 75, 76),
                    f(6, 43, 81, 100),
                ];
                T
            },
        },
        "CX" => Layout {
            top: 14,
            labels: &[
                (2, 2, "Level"),
                (2, 11, "N01"),
                (2, 16, "Factor 1"),
                (2, 33, "Operation"),
                (4, 2, "Extended"),
                (5, 2, "Factor 2"),
                (5, 52, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(3, 3, 7, 8),
                    f(3, 11, 9, 11),
                    f(3, 16, 12, 25),
                    f(3, 33, 26, 35),
                    f(6, 2, 36, 80),
                    f(6, 52, 81, 100),
                ];
                T
            },
        },
        "O" => Layout {
            top: 14,
            labels: &[
                (2, 2, "Filename"),
                (2, 15, "Type"),
                (2, 24, "Fetch"),
                (2, 32, "N01N02N03"),
                (2, 45, "EXCEPTName"),
                (4, 3, "---- Space -----"),
                (4, 23, "----- Skip -----"),
                (5, 3, "Before"),
                (5, 14, "After"),
                (5, 23, "Before"),
                (5, 34, "After"),
                (5, 45, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(3, 2, 7, 16),
                    f(3, 16, 17, 17),
                    f(3, 26, 18, 20),
                    f(3, 32, 21, 29),
                    f(3, 45, 30, 39),
                    f(6, 4, 40, 42),
                    f(6, 15, 43, 45),
                    f(6, 25, 46, 48),
                    f(6, 35, 49, 51),
                    f(6, 45, 81, 100),
                ];
                T
            },
        },
        "P" => Layout {
            top: 13,
            labels: &[
                (2, 14, "Field"),
                (2, 32, "Edit"),
                (2, 50, "End"),
                (2, 63, "Field"),
                (3, 2, "N01N02N03"),
                (3, 14, "Name"),
                (3, 32, "Code"),
                (3, 42, "B/A"),
                (3, 48, "Position"),
                (3, 63, "Format"),
                (5, 2, "Constant / Edit Word /"),
                (6, 2, "Date/Time Format"),
                (6, 45, "Comment"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(4, 2, 21, 29),
                    f(4, 14, 30, 43),
                    f(4, 33, 44, 44),
                    f(4, 43, 45, 45),
                    f(4, 49, 47, 51),
                    f(4, 65, 52, 52),
                    f(7, 2, 53, 80),
                    f(7, 45, 81, 100),
                ];
                T
            },
        },
        "I" => Layout {
            top: 12,
            labels: &[
                (2, 2, "Filename"),
                (2, 17, "Sequence"),
                (2, 31, "Number"),
                (2, 43, "Option"),
                (2, 55, "Record ID"),
                (4, 2, "Record Identification Codes"),
                (5, 2, "Position"),
                (5, 15, "Not"),
                (5, 22, "C/Z/D"),
                (5, 30, "Character"),
                (6, 2, "1"),
                (7, 2, "2"),
                (7, 42, "Comment"),
                (8, 2, "3"),
            ],
            fields: {
                const T: &[PField] = &[
                    f(3, 2, 7, 16),
                    f(3, 20, 17, 18),
                    f(3, 33, 19, 19),
                    f(3, 45, 20, 20),
                    f(3, 58, 21, 22),
                    f(6, 5, 23, 27),
                    f(6, 16, 28, 28),
                    f(6, 24, 29, 29),
                    f(6, 34, 30, 30),
                    f(7, 5, 31, 35),
                    f(7, 16, 36, 36),
                    f(7, 24, 37, 37),
                    f(7, 34, 38, 38),
                    f(8, 5, 39, 43),
                    f(8, 16, 44, 44),
                    f(8, 24, 45, 45),
                    f(8, 34, 46, 46),
                    f(8, 42, 81, 100),
                ];
                T
            },
        },
        _ => FREE,
    }
}

/// The spec letter a format puts in column 6, when it has one.
#[must_use]
pub fn spec_letter(fmt: &str) -> Option<char> {
    match fmt {
        "H" | "F" | "D" | "I" | "C" | "O" => fmt.chars().next(),
        "CX" => Some('C'),
        "P" => Some('O'),
        "DP" | "PF" | "LF" | "BC" => Some('A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families() {
        assert_eq!(SourceType::new("clle").family, Family::Cl);
        assert_eq!(SourceType::new("DSPF").checker(), Checker::Dds);
        assert_eq!(SourceType::new("RPGLE").first_column(), 6);
        assert_eq!(SourceType::new("RPGLE").upper(), Some(false));
        assert_eq!(SourceType::new("CMD").upper(), Some(true));
        assert_eq!(SourceType::new("").upper(), None);
        assert_eq!(SourceType::new("TXT").upper(), Some(false));
    }

    #[test]
    fn formats_of_records() {
        let r = SourceType::new("RPGLE");
        assert_eq!(r.format_of("     D*** comment"), "*");
        assert_eq!(r.format_of("     D QUSEI00                13     19"), "D");
        assert_eq!(r.format_of("     C                   EVAL      X = 1"), "CX");
        assert_eq!(r.format_of("     C     A             ADD       B             C"), "C");
        assert_eq!(r.format_of("     A          R X"), "**");
        let d = SourceType::new("DSPF");
        assert_eq!(d.format_of("     A          R X"), "DP");
        assert_eq!(SourceType::new("PF").format_of("     A  X"), "PF");
        assert_eq!(SourceType::new("CLP").format_of("PGM"), "**");
    }

    #[test]
    fn rulers() {
        let (l, r) = format_line("**", 1, 20);
        assert_eq!(l, "FMT **");
        assert_eq!(r, " ...+... 1 ...+... 2");
        let (_, r) = format_line("*", 1, 12);
        assert_eq!(r, "..... *. 1 .");
        let (l, r) = format_line("D", 5, 26);
        assert_eq!(l, "FMT D ");
        assert_eq!(r, ".DName+++++++++++ETDsF");
        let (_, r) = format_line("DP", 1, 50);
        assert!(r.starts_with(".....AAN01N02N03T.Name++++++RLen++TDpBLinPosFunc"));
    }

    #[test]
    fn layouts_fit_the_display() {
        for fmt in ["DP", "H", "F", "D", "C", "CX", "O", "P", "I", "**"] {
            let l = layout(fmt);
            for fl in l.fields {
                assert!(l.top + fl.row <= 20, "{fmt}");
                assert!(fl.col + (fl.to - fl.from) <= 80 || fl.to == 100, "{fmt}");
            }
            for (r, c, t) in l.labels {
                assert!(l.top + r <= 20 && c + t.len() <= 81, "{fmt}");
            }
        }
    }
}
