#!/usr/bin/env python3
"""Energy-guided short-utterance alignment; uncertain regions remain unverified. Outputs contain private transcripts; never commit them."""
import json,re,wave,os
from pathlib import Path
import numpy as np
from scipy.ndimage import binary_closing,binary_opening
from faster_whisper import WhisperModel
import argparse

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run',required=True,type=Path)
    p.add_argument('--model',default='small')
    args=p.parse_args()
    base=args.run.resolve()
    out=base/'segmented';out.mkdir(exist_ok=True)
    model=WhisperModel(args.model,device='cuda',compute_type='float16')
    manifest=[]
    def norm(s):return re.sub('[^а-я]','',s.lower().replace('ё','е'))
    def dist(a,b):
     row=list(range(len(b)+1))
     for i,ca in enumerate(a):
      nr=[i+1]
      for j,cb in enumerate(b):nr.append(min(nr[-1]+1,row[j+1]+1,row[j]+(ca!=cb)))
      row=nr
     return row[-1]
    for p in sorted((base/'recordings').glob('*.json')):
     if p.name.startswith('.'):continue
     m=json.loads(p.read_text())
     if m.get('excluded') or m['block']=='background':continue
     dest=out/(m['id']+'.json')
     if dest.exists():continue
     with wave.open(str(p.with_suffix('.wav'))) as w:x=np.frombuffer(w.readframes(w.getnframes()),dtype='<i2')
     n=len(x)//320;xx=x[:n*320].astype(float).reshape(n,320);db=20*np.log10(np.maximum(np.sqrt(np.mean(xx*xx,axis=1)),1)/32768)
     th=max(-53,min(-35,float(np.percentile(db,20))+6))
     act=binary_closing(db>th,structure=np.ones(23));act=binary_opening(act,structure=np.ones(6))
     e=np.diff(np.r_[False,act,False].astype(int));starts=np.where(e==1)[0];ends=np.where(e==-1)[0]
     candidates=[]
     for a,b in zip(starts,ends):
      if b-a<12:continue
      start=max(0,a*.02-.22);end=min(len(x)/16000,b*.02+.22)
      if end-start>8:continue
      clip=x[int(start*16000):int(end*16000)].astype(np.float32)/32768
      clip*=min(16.,.9/max(.001,float(np.max(np.abs(clip)))))
      segs,_=model.transcribe(clip,language='ru',beam_size=5,word_timestamps=True,condition_on_previous_text=False,vad_filter=False,temperature=0,no_speech_threshold=None,compression_ratio_threshold=None)
      words=[{'word':w.word,'start':round(start+w.start,3),'end':round(start+w.end,3),'p':w.probability} for s in segs for w in (s.words or [])]
      hits=[w for w in words if len(norm(w['word']))>=5 and dist(norm(w['word']),'рикардо')<=2 and w['end']-w['start']>=.12]
      candidates.append({'start':round(start,3),'end':round(end,3),'words':words,'hits':hits})
     result={'id':m['id'],'split':m['split'],'block':m['block'],'threshold':th,'candidates':candidates}
     dest.write_text(json.dumps(result,ensure_ascii=False,indent=2));print(m['split'],m['block'],'regions',len(candidates),'verified targets',sum(len(c['hits']) for c in candidates),flush=True)

if __name__ == "__main__":
    main()
