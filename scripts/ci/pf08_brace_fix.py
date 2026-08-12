from pathlib import Path

path = Path("crates/hermes-ui/src/files.rs")
text = path.read_text(encoding="utf-8")
old = """                    }
                    }
                }
            }
        }
    }
}
"""
new = """                    }
                }
            }
        }
    }
}
"""
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one duplicated Files section closure, found {count}")
text = text.replace(old, new, 1)
for old_attr, new_attr in [
    ('                        sandbox: "allow-scripts",', '                        "sandbox": "allow-scripts",'),
    ('                        loading: "lazy",', '                        "loading": "lazy",'),
]:
    if text.count(old_attr) != 1:
        raise SystemExit(f"expected one iframe attribute: {old_attr}")
    text = text.replace(old_attr, new_attr, 1)
path.write_text(text, encoding="utf-8")

contract = Path("crates/hermes-desktop/tests/preview_ui_contract.rs")
contract_text = contract.read_text(encoding="utf-8")
old_contract = '    assert!(ui.contains("sandbox: \\"allow-scripts\\""));'
new_contract = '    assert!(ui.contains("\\"sandbox\\": \\"allow-scripts\\""));'
if contract_text.count(old_contract) != 1:
    raise SystemExit("preview sandbox source contract changed")
contract.write_text(contract_text.replace(old_contract, new_contract, 1), encoding="utf-8")
