import json, glob, re, urllib.request, time, sys

import os

PROJECT = os.environ.get('CHAPTR_PROJECT') or (sys.argv[1] if len(sys.argv) > 1 else '')
if not PROJECT:
    sys.exit('set CHAPTR_PROJECT or pass a project folder holding transcripts/')
FS = sorted(glob.glob(os.path.join(PROJECT, 'transcripts', '*.json')))
if not FS:
    sys.exit(f'no transcripts under {PROJECT}')
URL = "http://localhost:8899/v1/chat/completions"

def hms(t): return f"{int(t)//60:02d}:{int(t)%60:02d}"

def window(fi, a, b):
    d = json.load(open(FS[fi]))
    return [s for s in d['segments'] if a <= s['start'] < b]

# Spread across quiet and busy stretches so a variant cannot win by only
# handling dense combat well.
WINDOWS = [(4,1800,2400),(4,2400,3000),(2,3000,3600),(2,600,1200),
           (7,600,1200),(9,1200,1800),(11,600,1200),(2,4200,4800)]

# Bare "Player"/"Speaker" is just as empty as "a player"; the first pass at this
# regex missed it and let a variant score 95% "named" on invented subjects.
VAGUE = re.compile(r"\b(a|the|another|one)?\s*(player|speaker|teammate|team-mate|enemy|"
                   r"opponent|character|hero|dude|guy)s?\b"
                   r"|\bsomeone\b|\bsomebody\b|\ba moment\b", re.I)
# Reporting speech instead of the event it implies.
QUOTEY = re.compile(r"\b(says?|said|asks?|asked|tells?|told|mentions?|mentioned|claims?|"
                    r"admits?|warns?|laughs?|calls?|called|suggests?|reveals?|threatens?)\b", re.I)
STOP = {"The","They","A","An","I","It","Their","There","This","That","He","She","We","His","Her",
        "One","Two","Three","After","Before","When","While","During","Both","All","Someone",
        "Player","Speaker","Team","Dude","Guy","Enemy","Both"}

def named(text):
    toks = re.findall(r"\b[A-Z][a-z]{2,}\b", text)
    return [t for t in toks if t not in STOP]

ANCHOR = re.compile(r"\b(mid|top|bot|lane|base|boss|patron|walker|guardian|shrine|tower|"
                    r"urn|jungle|camp|bridge|ult|ulted|hex|buff|orb|soul|blue|yellow|green|"
                    r"purple|red|kill|died|death)\b|\d", re.I)

def anchored(text):
    """Beat points at something identifiable: a name, a game noun or a number."""
    return bool(named(text)) or bool(ANCHOR.search(text))

def schema(maxi):
    return {"type":"object","properties":{"beats":{"type":"array","minItems":0,"maxItems":maxi,
      "items":{"type":"object","properties":{
        "t":{"type":"string","pattern":"^[0-9]{1,2}:[0-9]{2}$"},
        "text":{"type":"string","maxLength":90}},
       "required":["t","text"],"additionalProperties":False}}},"required":["beats"]}

def ask(system, user, maxi, temp=0.2):
    body = {"messages":[{"role":"system","content":system},{"role":"user","content":user}],
            "temperature":temp,"max_tokens":700,
            "chat_template_kwargs":{"enable_thinking":False},
            "response_format":{"type":"json_schema","json_schema":{"name":"beats","schema":schema(maxi)}}}
    r = urllib.request.urlopen(urllib.request.Request(URL, json.dumps(body).encode(),
        {"Content-Type":"application/json"}), timeout=300)
    return json.loads(json.loads(r.read())['choices'][0]['message']['content'])['beats']

def evaluate(name, build, maxi=6, show=False):
    beats_all, t0 = [], time.time()
    for fi,a,b in WINDOWS:
        segs = window(fi,a,b)
        if len(segs) < 3: continue
        lines = "\n".join(f"[{hms(s['start'])}] {s['text']}" for s in segs)
        sysmsg, usermsg = build(lines, hms(a), hms(b))
        try:
            bs = ask(sysmsg, usermsg, maxi)
        except Exception as e:
            print("  fail:", e); bs = []
        for x in bs: x['_w'] = f"f{fi}@{hms(a)}"
        beats_all += bs
        if show:
            print(f"  --- f{fi} {hms(a)}-{hms(b)} ({len(segs)} segs)")
            for x in bs: print(f"      [{x['t']}] {x['text']}")
    n = len(beats_all) or 1
    vague = sum(1 for x in beats_all if VAGUE.search(x['text']))
    quotey = sum(1 for x in beats_all if QUOTEY.search(x['text']))
    withname = sum(1 for x in beats_all if named(x['text']))
    words = sum(len(x['text'].split()) for x in beats_all)/n
    # A vague noun is only a problem when nothing else anchors the beat, so
    # "they killed the enemy Yamato" passes and "an enemy was eliminated" fails.
    good = sum(1 for x in beats_all if anchored(x['text'])
               and not QUOTEY.search(x['text'])
               and len(x['text'].split()) >= 4)
    el = time.time()-t0
    print(f"{name:22} {len(beats_all):3} beats  vague {100*vague/n:3.0f}%  "
          f"quote {100*quotey/n:3.0f}%  named {100*withname/n:3.0f}%  "
          f"{words:.1f}w  GOOD {100*good/n:3.0f}%  {el:.0f}s")
    return beats_all
