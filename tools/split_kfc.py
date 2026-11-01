import os, sys

SRC = "enshrouded.kfc_resources"
CHUNK = 50_000_000

total = os.path.getsize(SRC)
print(f"source: {SRC} = {total} bytes")
written = 0
i = 0
with open(SRC, "rb") as f:
    while True:
        data = f.read(CHUNK)
        if not data:
            break
        name = f"seg{i:02d}.bin"
        with open(name, "wb") as out:
            out.write(data)
        written += len(data)
        print(f"{name}: {len(data)} bytes (total {written}/{total})")
        i += 1
print(f"DONE: {i} parts, {written} bytes")
if written != total:
    print("ERROR: byte count mismatch!")
    sys.exit(1)
