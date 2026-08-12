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
path.write_text(text.replace(old, new, 1), encoding="utf-8")
