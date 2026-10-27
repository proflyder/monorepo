#!/usr/bin/env python3
"""Synthesize and transcribe a fixed phrase without playback or HA mutations.

Run with the Wyoming Python environment, e.g. inside the Piper Pod. Audio is
kept in memory; no user recording is accessed or printed.
"""
import asyncio
import array
import time
from wyoming.client import AsyncTcpClient
from wyoming.tts import Synthesize, SynthesizeVoice
from wyoming.audio import AudioStart, AudioChunk, AudioStop
from wyoming.asr import Transcribe, Transcript


async def check():
    data = bytearray()
    start = time.monotonic()
    async with AsyncTcpClient("127.0.0.1", 10200, read_timeout=60) as client:
        await client.write_event(Synthesize("Рикардо, сколько сейчас времени?",
            SynthesizeVoice(name="ru_puriso2-medium", language="ru")).event())
        while True:
            event = await client.read_event()
            if event is None:
                raise AssertionError("Piper disconnected before audio-stop")
            if AudioStart.is_type(event.type):
                fmt = AudioStart.from_event(event)
            elif AudioChunk.is_type(event.type):
                data.extend(AudioChunk.from_event(event).audio)
            elif AudioStop.is_type(event.type):
                break
    assert len(data) > 1000
    assert fmt.width == 2 and fmt.channels == 1
    print(f"Puriso synthesis PASS: {len(data)} bytes, {time.monotonic()-start:.2f}s")
    samples = array.array("h", data)
    # Linear resampling of signed PCM16 for ASR; no audio file or player needed.
    count = int(len(samples) * 16000 / fmt.rate)
    converted = array.array("h")
    for i in range(count):
        pos = i * fmt.rate / 16000
        n = min(int(pos), len(samples)-1)
        nxt = min(n+1, len(samples)-1)
        converted.append(round(samples[n] + (samples[nxt]-samples[n])*(pos-n)))
    pcm = converted.tobytes()
    for name, port in [("GigaAM", 10305), ("Whisper", 10300)]:
        start = time.monotonic()
        async with AsyncTcpClient("127.0.0.1", port, read_timeout=120) as client:
            await client.write_event(Transcribe(language="ru").event())
            await client.write_event(AudioStart(rate=16000, width=2, channels=1).event())
            for offset in range(0, len(pcm), 6400):
                await client.write_event(AudioChunk(rate=16000, width=2, channels=1,
                    audio=pcm[offset:offset+6400]).event())
            await client.write_event(AudioStop().event())
            while True:
                event = await client.read_event()
                if event is None:
                    raise AssertionError(f"{name}: disconnected before transcript")
                if Transcript.is_type(event.type):
                    text = Transcript.from_event(event).text.lower()
                    assert "врем" in text or "сколько" in text, f"{name}: unexpected fixed-phrase transcription"
                    print(f"{name} transcription PASS: {time.monotonic()-start:.2f}s")
                    break


if __name__ == "__main__":
    asyncio.run(check())
