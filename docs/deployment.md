# Free Hosting Options

This app is a Dioxus 0.7 fullstack Docker container (axum server + wasm client). The options below require no code changes — just the existing `Dockerfile` and the two runtime env vars (`OPENAI_API_KEY`, `OPENCODE_API_KEY`).

## Fly.io (recommended)

- **Free tier:** 3 shared-cpu-1x 256MB VMs, 3GB storage, 160GB egress/month
- **Deploy:** `fly launch` reads your Dockerfile, `fly secrets set OPENAI_API_KEY=... OPENCODE_API_KEY=...`
- **Pro:** global edge network, fast cold starts, mature platform for Rust apps
- **Con:** requires a credit card to sign up (but stays within the free allowance)

## Render

- **Free tier:** Web services sleep after 15 min idle (wakes on request), 512MB RAM
- **Deploy:** connect GitHub repo → pick Docker → add env vars
- **Pro:** no credit card needed for free tier, dead simple
- **Con:** cold start takes ~5–10s on first request after sleep

## Google Cloud Run

- **Free tier:** 2M requests/month, 360k vCPU-seconds, 360k GB-seconds, 1GB egress
- **Deploy:** `gcloud builds submit --tag gcr.io/...` then `gcloud run deploy`
- **Pro:** most generous raw compute free tier, scales to zero instantly
- **Con:** more manual setup (GCP project, gcloud CLI, container registry)

## Koyeb

- **Free tier:** 1 app with 0.5GB RAM, 1GB storage, 100GB bandwidth, always-on
- **Deploy:** GitHub repo → Dockerfile → add env vars
- **Pro:** no credit card needed, always-on (no cold start), simple

## Railway

- **Free tier:** \$5 credit (roughly ~500 hours/month of a small VM)
- The free offering has been shrinking; fine for a demo but not truly "free forever"
