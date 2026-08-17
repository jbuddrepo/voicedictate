# VoiceDictate Remote Transcription Setup

This guide explains how to run Whisper on another computer and connect VoiceDictate to it. It is written for a first-time server setup and applies to a home server, mini PC, Linux VM, NAS with Docker, or hosted Linux machine.

VoiceDictate continues to run on the Windows computer where you dictate. It records the microphone, sends one temporary WAV file to the server you select, receives text, deletes the WAV, and pastes the text into the focused application.

## Read this before starting

- The Windows computer and Whisper server must be able to reach each other. They may be on the same LAN, or connected to the same private Tailscale network. They do not have to be in the same building when Tailscale is used.
- Never forward port `8080`, port `11434`, or an unprotected Whisper endpoint through an internet router. The basic whisper.cpp server does not provide user authentication or public-internet security.
- Publicly hosted servers must use HTTPS and authentication through a properly configured reverse proxy. VoiceDictate rejects plain HTTP to public IP addresses.
- Remote transcription sends microphone audio to the configured server. Local mode does not.
- Ollama and Whisper serve different purposes. Ollama can clean completed text; a Whisper server transcribes audio. They can run on the same machine, but Ollama alone is not required for transcription.
- Start with CPU processing. GPU passthrough, AMD Vulkan, and NVIDIA CUDA can be added after the CPU server works.

## Recommended starting resources

For a Linux VM, begin with 4 CPU cores and 4 GB RAM for the multilingual Base model. Give it more CPU only if those cores are genuinely available on the host. The Small model benefits from additional CPU and memory but will take longer. Avoid assigning every K11 core to the VM when Proxmox is also running other workloads.

Model choices:

- `tiny`: fastest, lowest accuracy.
- `base`: recommended first test and a good speed/accuracy balance.
- `small`: better accuracy, more CPU and memory, slower response.
- Models ending in `.en` are English-only. Models without `.en` are multilingual.

## K11 and Proxmox preparation

The simplest arrangement is to install the server inside a Linux VM such as AIVM01.

1. Open the Proxmox console for the VM.
2. Confirm that it is a 64-bit Ubuntu or Debian-based Linux VM:

   ```bash
   uname -m
   cat /etc/os-release
   ```

3. Prefer a bridged network adapter so the VM receives its own LAN address. Display its addresses:

   ```bash
   hostname -I
   ```

4. Write down the LAN address, for example `192.168.1.50`. Do not use `127.0.0.1`; that address means the VM itself.
5. If AIVM01 already runs Ollama, leave it in place. Whisper uses port `8080`, while Ollama normally uses port `11434`.

## Step 1: Install Docker Engine

Skip this step if `sudo docker run hello-world` already works. These commands are for Ubuntu. Debian and other distributions should use the matching instructions in the [official Docker Engine installation guide](https://docs.docker.com/engine/install/).

```bash
sudo apt update
sudo apt install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
```

Add Docker's Ubuntu repository:

```bash
sudo tee /etc/apt/sources.list.d/docker.sources >/dev/null <<EOF
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
Components: stable
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/docker.asc
EOF
```

Install and verify Docker:

```bash
sudo apt update
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo systemctl enable --now docker
sudo docker run --rm hello-world
```

Docker publishes ports through its own firewall rules. Do not assume an `ufw` rule alone hides a published container port. Bind the container to the exact LAN or Tailscale address as shown below, and never create a router port-forward.

## Step 2: Download a Whisper model

Create a permanent folder. The model remains there when the container is updated or restarted.

```bash
sudo mkdir -p /opt/voicedictate-whisper/models
cd /opt/voicedictate-whisper
sudo docker run --rm \
  -v "$PWD/models:/models" \
  ghcr.io/ggml-org/whisper.cpp:main \
  "./models/download-ggml-model.sh base /models"
```

Confirm that the model exists:

```bash
ls -lh /opt/voicedictate-whisper/models/ggml-base.bin
```

The container and server are provided by the official [whisper.cpp project](https://github.com/ggml-org/whisper.cpp), which includes an HTTP transcription server.

## Step 3A: Run it on the same local network

Replace the example address with the VM's LAN address from `hostname -I`:

```bash
VOICE_SERVER_IP="192.168.1.50"
sudo docker run -d \
  --name voicedictate-whisper \
  --restart unless-stopped \
  -p "${VOICE_SERVER_IP}:8080:8080" \
  -v /opt/voicedictate-whisper/models:/models:ro \
  ghcr.io/ggml-org/whisper.cpp:main \
  "whisper-server --host 0.0.0.0 --port 8080 -m /models/ggml-base.bin -t 8"
```

Check that it is running:

```bash
sudo docker ps
sudo docker logs --tail 100 voicedictate-whisper
curl "http://${VOICE_SERVER_IP}:8080/"
```

On the Windows computer, open PowerShell and test the port:

```powershell
Test-NetConnection -ComputerName 192.168.1.50 -Port 8080
```

`TcpTestSucceeded : True` means Windows can reach the server.

## Step 3B: Use Tailscale instead of the LAN

Use this option when the Windows computer and server are in different locations, or when you want a private stable connection. Install Tailscale directly in the Linux VM rather than only on the Proxmox host.

```bash
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
tailscale ip -4
```

Open the authentication link displayed by `tailscale up`. The Windows computer must also be signed into the same tailnet. The official instructions are in [Install Tailscale on Linux](https://tailscale.com/docs/install/linux).

Stop and replace the LAN-bound container if it is already running:

```bash
sudo docker rm -f voicedictate-whisper
VOICE_SERVER_IP="$(tailscale ip -4)"
sudo docker run -d \
  --name voicedictate-whisper \
  --restart unless-stopped \
  -p "${VOICE_SERVER_IP}:8080:8080" \
  -v /opt/voicedictate-whisper/models:/models:ro \
  ghcr.io/ggml-org/whisper.cpp:main \
  "whisper-server --host 0.0.0.0 --port 8080 -m /models/ggml-base.bin -t 8"
```

The VoiceDictate URL will look like `http://100.x.y.z:8080`. This private HTTP connection is permitted for Tailscale addresses. Do not use Tailscale Funnel for this server.

## Step 4: Configure VoiceDictate

1. Install or open VoiceDictate 0.2.0 or newer on Windows.
2. Open **Settings**.
3. Under **Transcription location**, choose one of:
   - **My server or online provider**: fail with an error if the server is unavailable.
   - **Automatic — remote, then local fallback**: use bundled local Whisper if the server is unavailable.
4. Set **Server type** to **whisper.cpp server**.
5. Enter the server root URL:
   - LAN example: `http://192.168.1.50:8080`
   - Tailscale example: `http://100.x.y.z:8080`
6. Leave **API key** blank for the basic private whisper.cpp server.
7. Start with a 90-second timeout.
8. Select **Test transcription server**. It should show **Connected**.
9. Select **Save**.
10. Dictate a short sentence and confirm that it is pasted.
11. If using Automatic mode, stop the server with `sudo docker stop voicedictate-whisper` and dictate again. VoiceDictate should report that it used local Whisper.

VoiceDictate accepts either the server root URL or a URL ending in `/inference`.

## Managing the server

```bash
# View status and recent output
sudo docker ps
sudo docker logs --tail 100 voicedictate-whisper

# Stop and start it
sudo docker stop voicedictate-whisper
sudo docker start voicedictate-whisper

# Restart it
sudo docker restart voicedictate-whisper

# Remove the container but keep the model
sudo docker rm -f voicedictate-whisper
```

To switch to the multilingual Small model, download `small`, remove the container, and recreate it with `/models/ggml-small.bin`. Test speed before keeping the larger model.

## Other users and other hardware

People without a K11 have the following choices:

- Keep **Local** mode and use bundled Whisper for free.
- Run the same Docker container on another Linux desktop, mini PC, home server, or compatible NAS.
- Build and run whisper.cpp natively without Docker using the instructions in [BUILDING.md](BUILDING.md).
- Configure an online service that supports the OpenAI-compatible `/v1/audio/transcriptions` API. Select **OpenAI-compatible audio API**, enter its HTTPS base URL, model name, and the user's own API key.

The VoiceDictate Windows application never supplies a shared cloud account or shared API key. Online-provider charges, retention rules, file limits, and privacy terms belong to the provider selected by the user.

## Troubleshooting

### Connection refused or the Test button fails

- Run `sudo docker ps` and check that the container is listed.
- Run `sudo docker logs --tail 100 voicedictate-whisper` and look for a missing model or invalid option.
- Confirm the URL uses the VM address, not `127.0.0.1`.
- Run `Test-NetConnection` from Windows.
- Confirm both devices are on the same LAN or signed into the same Tailscale tailnet.
- Check Proxmox, VM, cloud, and Docker firewall rules.

### Test works but transcription times out

- Increase the VoiceDictate timeout.
- Start with Base instead of Small.
- Reduce other Ollama or VM workloads competing for CPU and RAM.
- Check server logs while dictating.

### English works but other languages do not

- Use `base`, not `base.en`.
- Leave VoiceDictate language on Auto, or choose the correct language explicitly.

### Online provider returns 401

- Re-enter the API key.
- Confirm the key is for the selected provider and the base URL is correct.
- VoiceDictate stores the key in Windows Credential Manager, not in its settings file.

### Online provider returns 404

- Use the provider's API base URL, normally ending in `/v1`, rather than a dashboard webpage.
- VoiceDictate adds `/audio/transcriptions` to a `/v1` base URL automatically.

## Security checklist

- [ ] No router port-forward exists for `8080` or `11434`.
- [ ] LAN access is limited to trusted devices, or Tailscale is used.
- [ ] Public endpoints use HTTPS and authentication.
- [ ] The user understands that Remote and Automatic modes send microphone audio.
- [ ] Automatic fallback is enabled when uninterrupted dictation matters.
- [ ] Server and container updates are applied periodically.

