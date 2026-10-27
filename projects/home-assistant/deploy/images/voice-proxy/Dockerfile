FROM python:3.14.9-slim-trixie
WORKDIR /app
COPY voice_proxy.py /app/voice_proxy.py
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1
USER 1000:1000
ENTRYPOINT ["python3", "/app/voice_proxy.py"]
