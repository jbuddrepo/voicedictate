# VoiceDictate Installation Video Checklist

This is a suggested recording order for a VoiceDictate 0.2.0 installation and remote-Whisper demonstration. Review [REMOTE-TRANSCRIPTION.md](REMOTE-TRANSCRIPTION.md) before recording the server portion.

## Before recording

- Use VoiceDictate 0.2.0 and show **Help & About** so viewers can confirm their version.
- Close unrelated applications and notifications.
- Do not show API keys, passwords, Tailscale authentication links, signing keys, or private account pages.
- A LAN or Tailscale IP can be blurred if desired. Never show or recommend a router port-forward.
- Prepare one short sentence for every transcription test so viewers can compare the results.
- Explain that the Windows beta is unsigned and may trigger Windows SmartScreen.

## Part 1: Install VoiceDictate on Windows

1. Download the installer from the GitHub Releases page.
2. Run `VoiceDictate-Setup-0.2.0.exe`.
3. If SmartScreen appears, show **More info**, verify the filename, and select **Run anyway**.
4. Start VoiceDictate.
5. Open **Help & About** and show `VoiceDictate version 0.2.0`.
6. Open **Settings** and choose the microphone.
7. Leave transcription on **Local — free and private**.
8. Save the settings.
9. Click into Notepad, press the dictation shortcut, speak, stop, and show the pasted text.
10. Explain that Local mode needs no server, account, API key, subscription, or internet connection.

## Part 2: Explain the three choices

Show **Help & About** and briefly describe:

- **Local Whisper:** audio stays on the Windows PC.
- **Your own server:** audio goes only to the LAN or Tailscale server selected by the user.
- **Online provider:** audio goes to the external provider selected and paid for by the user.

Explain that Ollama cleanup is separate. It receives completed text, while the Whisper transcription server receives audio.

## Part 3A: Prepare another Windows PC—the common setup

1. On the more powerful Windows PC, create `VoiceDictate-Whisper-Server` under the user's profile.
2. Download the official `whisper-bin-x64.zip` release and extract it.
3. Download the multilingual `ggml-base.bin` model.
4. Start `whisper-server.exe` on port `8080` using the exact PowerShell steps in `REMOTE-TRANSCRIPTION.md`.
5. When Windows Firewall prompts, select **Private networks only**.
6. Show the whisper.cpp page at `http://127.0.0.1:8080` on the server PC.
7. From the VoiceDictate PC, show `Test-NetConnection` succeeding against the server's LAN or Tailscale address.
8. State clearly: **Do not forward port 8080 or 11434 through the router.**

Explain that the second Windows PC does not need Python, Ollama, Docker, Rust, Visual Studio, or VoiceDictate itself. It needs the official whisper.cpp ZIP, a model, and private network access.

## Part 3B: Optional K11 or Linux-server setup

1. Show the Linux VM console and identify the OS with:

   ```bash
   uname -m
   cat /etc/os-release
   ```

2. Display the VM's LAN address with `hostname -I`.
3. Install Docker using the exact steps in `REMOTE-TRANSCRIPTION.md`.
4. Verify Docker using:

   ```bash
   sudo docker run --rm hello-world
   ```

5. Download the multilingual Base model.
6. Start the official whisper.cpp server on port `8080`.
7. Show that it remains running with:

   ```bash
   sudo docker ps
   sudo docker logs --tail 100 voicedictate-whisper
   ```

8. State clearly: **Do not forward port 8080 or 11434 through the router.**

## Part 4: Choose LAN or Tailscale

For a same-network demonstration, use the VM's private LAN address such as `192.168.x.x`.

For access between locations:

1. Install Tailscale inside the Linux VM.
2. Connect the Windows PC and Linux VM to the same tailnet.
3. Use `tailscale ip -4` to obtain the private server address.
4. Do not record the one-time authentication URL.
5. Explain that the devices do not need to be on the same physical network when Tailscale connects them privately.

## Part 5: Connect VoiceDictate to whisper.cpp

1. Open **Settings → Transcription location**.
2. Choose **My server or online provider**.
3. Choose **whisper.cpp server**.
4. Enter the server root URL, such as `http://192.168.1.50:8080` or the private Tailscale address.
5. Leave the API key blank for a private basic whisper.cpp server.
6. Select **Test transcription server** and show **Connected**.
7. Save settings.
8. Dictate into Notepad again and compare the processing time.
9. Explain that the temporary WAV is deleted after transcription.

## Part 6: Demonstrate Automatic fallback

1. Change the method to **Automatic — remote, then local fallback**.
2. Confirm remote dictation works.
3. Stop the server:

   ```bash
   sudo docker stop voicedictate-whisper
   ```

4. Dictate again.
5. Show the VoiceDictate message explaining that local Whisper was used.
6. Restart the server:

   ```bash
   sudo docker start voicedictate-whisper
   ```

This demonstrates that users do not lose dictation when their private server is offline.

## Part 7: Mention online providers

Do not enter a real API key on camera. Explain that users can select **OpenAI-compatible audio API** and provide their own:

- HTTPS API base URL
- Transcription model name
- API key
- Provider account and payment method, when required

State that VoiceDictate does not provide a shared key or free hosted service. Provider pricing, retention, privacy, and limits are controlled by that provider.

## Final statements for the video

- VoiceDictate is local-first and Local mode remains free and offline.
- Remote transcription is optional.
- LAN and Tailscale servers belong to the user.
- Microphone audio is never sent to an Ollama text-cleanup model.
- Never expose raw Whisper or Ollama ports to the public internet.
- The source, privacy statement, checksums, and server instructions are available in the GitHub repository and release files.

## Pre-publication review

- [ ] Every displayed installer and About screen says 0.2.0.
- [ ] Download links point to the correct GitHub release assets.
- [ ] No credentials or private authentication links appear in the recording.
- [ ] Local, Remote, and Automatic modes are each explained accurately.
- [ ] The privacy difference between sending audio and sending transcript text is clear.
- [ ] The no-port-forwarding warning is spoken and visible.
- [ ] The server commands were tested on the actual K11/AIVM01 configuration.
- [ ] Automatic fallback is demonstrated.
