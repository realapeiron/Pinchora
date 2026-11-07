"""Repack original pixels, never resample. Uses Python's standard library only.
Run from any directory: python3 web/scripts/build-reef.py. Output is deterministic.
"""
from pathlib import Path
from reef_png import Image
import hashlib, io, json, re, sys

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'web/public/assets/reef'
OUT.mkdir(parents=True, exist_ok=True)
sources = [Path(__file__), Path(__file__).with_name('reef_png.py'), ROOT/'assets/sprites/atlas.png', ROOT/'assets/sprites/atlas.json', ROOT/'crates/pinchora-render/src/character/palette.rs', *sorted((ROOT/'assets/reef/looks').glob('*.json')), *sorted((ROOT/'assets/reef/looks').glob('*.png'))]
source_hash = hashlib.sha256(b''.join(hashlib.sha256(p.read_bytes()).digest() for p in sources)).hexdigest()
stamp = OUT / '.source-hash'
if stamp.exists() and stamp.read_text() == source_hash and (OUT/'manifest.json').exists():
    old = json.loads((OUT/'manifest.json').read_text())
    url_file = ROOT/'web/src/reef/manifest-url.json'
    if url_file.exists() and (ROOT/'web/src/reef/palettes.json').exists() and (ROOT/'web/public'/json.loads(url_file.read_text()).lstrip('/')).exists() and all((ROOT/'web/public'/p['url'].lstrip('/')).exists() for p in old['pages'].values()):
        print('Reef asset pages are up to date')
        sys.exit(0)
manifest = {'version': 1, 'pages': {}, 'sprites': {}, 'looks': {}}

def page(im):
    stream = io.BytesIO()
    im.save(stream, format='PNG', optimize=True)
    data = stream.getvalue()
    key = hashlib.sha256(data).hexdigest()[:20]
    (OUT / (key + '.png')).write_bytes(data)
    manifest['pages'][key] = {'url': '/assets/reef/' + key + '.png', 'bytes': len(data), 'width': im.width, 'height': im.height}
    return key

atlas = Image.open(ROOT / 'assets/sprites/atlas.png').convert('RGBA')
index = json.loads((ROOT / 'assets/sprites/atlas.json').read_text())
# Co-use groups, shelf-packed into bounded 512px pages. Every crop is verified after encoding.
for prefix in sorted({name.split('_')[0] for name in index}):
    pending = []
    sheet = Image.new('RGBA', (512, 512))
    x = y = row = 0
    def flush():
        if not pending: return
        key = page(sheet)
        decoded = Image.open(OUT / (key + '.png')).convert('RGBA')
        for name, rect, original in pending:
            px, py, w, h = rect
            assert decoded.crop((px, py, px+w, py+h)).tobytes() == original.tobytes(), name
            manifest['sprites'][name] = {'page': key, 'rect': rect}
    for name in sorted(n for n in index if n.split('_')[0] == prefix):
        e = index[name]; w, h = e['w'], e['h']
        assert w <= 512 and h <= 512
        if x+w > 512: x = 0; y += row; row = 0
        if y+h > 512:
            flush(); pending = []; sheet = Image.new('RGBA', (512, 512)); x = y = row = 0
        original = atlas.crop((e['x'], e['y'], e['x']+w, e['y']+h))
        sheet.paste(original, (x,y))  # NO alpha mask: preserves even transparent RGB exactly.
        pending.append((name, [x,y,w,h], original)); x += w; row = max(row,h)
    flush()

for path in sorted((ROOT / 'assets/reef/looks').glob('*_metadata.json')):
    meta = json.loads(path.read_text())
    sheet = Image.open(path.with_name(path.name.replace('_metadata.json','_sheet.png'))).convert('RGBA')
    assert sheet.size == (768,512) and meta['cell_size'] == [96,128]
    assert meta['rows'] == ['SW','SE','NW','NE'] and len(meta['frames']) == 32
    directions = {d: page(sheet.crop((0,r*128,768,(r+1)*128))) for r,d in enumerate(meta['rows'])}
    frames = [{k:f[k] for k in ('direction','pose','foot_anchor','seat_contact','accessory_anchors','hands')} for f in meta['frames']]
    manifest['looks'][meta['look']] = {'directions':directions, 'frames':frames}
assert len(manifest['looks']) == 24
encoded = json.dumps(manifest, separators=(',', ':'), sort_keys=True).encode()
(OUT / 'manifest.json').write_bytes(encoded)
manifest_name = hashlib.sha256(encoded).hexdigest()[:20] + '.json'
(OUT / manifest_name).write_bytes(encoded)
(ROOT / 'web/src/reef/manifest-url.json').write_text(json.dumps('/assets/reef/' + manifest_name)+'\n')
# Generate the hand-tuned palette data directly from the bot, avoiding a second colour catalogue.
source = (ROOT / 'crates/pinchora-render/src/character/palette.rs').read_text()
palettes = {}
for key in ['SKIN_TONES','HAIR_COLORS','OUTFIT_COLORS','ACCESSORY_TINTS']:
    block = source.split('pub const '+key+':')[1].split('\n];')[0]
    values = []
    for line in block.splitlines()[1:]:
        nums = [int(x,16) for x in re.findall(r'0x([0-9a-f]+)',line)]
        if nums: values.append({'ramp': nums} if 'hand(' in line else {'rgb':nums[0]})
    palettes[key] = values
(ROOT / 'web/src/reef/palettes.json').write_text(json.dumps(palettes, indent=2)+'\n')
# Remove only builder-owned obsolete hashed files, never original source art.
keep = {key + '.png' for key in manifest['pages']} | {manifest_name}
for stale in OUT.iterdir():
    if re.fullmatch(r'[a-f0-9]{20}\.(png|json)', stale.name) and stale.name not in keep:
        stale.unlink()
stamp.write_text(source_hash)
print(f"Reef: {len(index)} exact crops, 24 looks, {len(manifest['pages'])} pages; manifest {len(encoded):,} bytes")
