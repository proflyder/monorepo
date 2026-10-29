#!/usr/bin/env python3
"""GPU transcription cache for screening background audio. Outputs contain private transcripts; never commit them."""
import json,os,time,wave
from pathlib import Path
import numpy as np
from faster_whisper import WhisperModel
import argparse

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--run',required=True,type=Path)
    p.add_argument('--model',default='small')
    args=p.parse_args()
    base=args.run.resolve()
    out=base/'asr';out.mkdir(exist_ok=True)
    model=WhisperModel(args.model,device='cuda',compute_type='float16')
    for p in sorted((base/'recordings').glob('*.json')):
     if p.name.startswith('.'):continue
     m=json.loads(p.read_text())
     if m.get('excluded'):continue
     dest=out/(m['id']+'.json')
     if dest.exists():continue
     with wave.open(str(p.with_suffix('.wav'))) as w:x=np.frombuffer(w.readframes(w.getnframes()),dtype='<i2').astype(np.float32)/32768
     # ASR-only gain; saved training PCM remains untouched.
     x=x*min(12.,.9/max(.001,float(np.max(np.abs(x)))))
     print('START',m['split'],m['block'],flush=True)
     segs,info=model.transcribe(x,language='ru',beam_size=5,word_timestamps=True,condition_on_previous_text=False,vad_filter=True,vad_parameters=dict(threshold=.25,min_silence_duration_ms=450,speech_pad_ms=180),temperature=0)
     result=[]
     for s in segs:
      result.append({'start':s.start,'end':s.end,'text':s.text,'avg_logprob':s.avg_logprob,'no_speech_prob':s.no_speech_prob,'words':[{'word':w.word,'start':w.start,'end':w.end,'probability':w.probability} for w in (s.words or [])]})
     dest.write_text(json.dumps({'segments':result},ensure_ascii=False,indent=2));os.chmod(dest,0o600)
     print('DONE',m['split'],m['block'],'segments',len(result),flush=True)

if __name__ == "__main__":
    main()
