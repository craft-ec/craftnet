import os
import re

root = "/Users/onlyabrak/dev/craftec/craftnet"

# 1. Remove "crates/app" from workspace members in Cargo.toml
root_cargo = os.path.join(root, "Cargo.toml")
with open(root_cargo, "r") as f:
    c = f.read()

c = re.sub(r'\s*"crates/app",\n', '\n', c)
c = c.replace('craftnet-app = { path = "crates/app" }', 'craftec-app = { workspace = true }')
with open(root_cargo, "w") as f:
    f.write(c)

# 2. Iterate and replace in all Cargo.toml
for dirpath, dirnames, filenames in os.walk(root):
    if '/target' in dirpath or '/.git' in dirpath:
        continue
    for filename in filenames:
        if filename == 'Cargo.toml':
            path = os.path.join(dirpath, filename)
            with open(path, "r") as f:
                c = f.read()
            c = c.replace('craftnet-app =', 'craftec-app =')
            with open(path, "w") as f:
                f.write(c)

# 3. Replace in .rs files
for dirpath, dirnames, filenames in os.walk(root):
    if '/target' in dirpath or '/.git' in dirpath:
        continue
    for filename in filenames:
        if filename.endswith('.rs'):
            path = os.path.join(dirpath, filename)
            with open(path, "r") as f:
                c = f.read()
            old_c = c
            
            # Change imports
            c = c.replace('craftnet_app::', 'craftec_app::')
            c = c.replace('use craftnet_app::', 'use craftec_app::')
            
            # Simple replacements, will fix compilation later if needed
            
            if old_c != c:
                with open(path, "w") as f:
                    f.write(c)
print("Done fixing app.")
