from pathlib import Path
import re,json,sys
root=Path(sys.argv[1])
# Mask Rust strings and comments, keeping offsets and line numbers unchanged.
def mask(s):
    pat=re.compile(r'//[^\n]*|/\*|r(\#*)"|"|\'(?:\\.|[^\'\\\n])\'')
    out=list(s);i=0
    while m:=pat.search(s,i):
        a=m.start(); token=m.group()
        if token=='/*':
            depth=1;b=m.end()
            while depth:
                n=re.search(r'/\*|\*/',s[b:])
                if not n: b=len(s);break
                b+=n.end();depth+=1 if n.group()=='/*' else -1
        elif token.startswith('r'):
            end='"'+m.group(1); p=s.find(end,m.end());b=len(s) if p<0 else p+len(end)
        elif token=='"':
            b=m.end()
            while b<len(s):
                if s[b]=='\\':b+=2
                elif s[b]=='"':b+=1;break
                else:b+=1
        else:b=m.end()
        for j in range(a,b):
            if s[j]!='\n':out[j]=' '
        i=b
    return ''.join(out)
rows=[]
for p in sorted([p for area in ['src','crates/xsht/src'] for p in (root/area).rglob('*.rs')]):
    orig=p.read_text();s=mask(orig)
    ts=list(re.finditer(r'[A-Za-z_][A-Za-z_0-9]*|::|=>|[^\s]',s))
    vals=[m.group() for m in ts]; pairs={};stack=[]
    for i,t in enumerate(vals):
        if t in '{([':stack.append((t,i))
        elif t in '})]' and stack and stack[-1][0]=={'}':'{',')':'(',']':'['}[t]:
            _,j=stack.pop();pairs[j]=i
    for i,t in enumerate(vals):
        if t!='match':continue
        j=i+1
        while j<len(vals) and vals[j]!='{':
            if vals[j] in '([' and j in pairs:j=pairs[j]
            j+=1
        if j not in pairs:continue
        end=pairs[j];k=j+1;arms=[];pattern=k
        while k<end:
            if vals[k]=='=>':
                pp=vals[pattern:k];body=k+1; q=body
                if vals[q]=='{' and q in pairs:q=pairs[q]+1
                else:
                    while q<end and vals[q]!=',':
                        if vals[q] in '({[' and q in pairs:q=pairs[q]
                        q+=1
                arms.append((pp,body,q)); k=q+1 if q<end and vals[q]==',' else q;pattern=k
            elif vals[k] in '({[' and k in pairs:k=pairs[k]+1
            else:k+=1
        kinds=set();variants=[];wild=[]
        for pp,a,b in arms:
            for n in range(len(pp)-2):
                if pp[n] in ('ArenaExprKind','ArenaExprTag','BuildExprRow','FullTag') and pp[n+1]=='::':
                    kinds.add(pp[n]);variants.append(pp[n]+'::'+pp[n+2])
            if pp==['_'] or pp and pp[0]=='_' and 'if' not in pp:
                wild.append(orig[ts[a].start():ts[max(a,b-1)].end()])
        if not kinds:continue
        pos=ts[i].start();line=orig.count('\n',0,pos)+1
        fns=list(re.finditer(r'\bfn\s+(\w+)',s[:pos]));fn=fns[-1].group(1) if fns else None
        rows.append(dict(file=str(p.relative_to(root)),line=line,function=fn,kinds=sorted(kinds),variants=sorted(set(variants)),wildcard=wild,span=[pos,ts[end].end()]))
json.dump(rows,sys.stdout,indent=2)
