GRUBCRAFT for Mac
=================

Starting the game
-----------------
1. Unzip, then drag Grubcraft.app into your Applications folder
   (or leave it where it is).
2. The first time only: right-click (or Control-click) Grubcraft.app,
   choose Open, then click Open again to confirm.
   Grubcraft is not from the App Store, so a plain double-click is
   blocked the first time. After this, double-click works.
If the Open button is missing: open System Settings, Privacy & Security,
scroll down and click "Open Anyway" next to Grubcraft.

Local network access
--------------------
The first time you host or join, your Mac asks if Grubcraft can find
devices on your local network. Click Allow. Without it, LAN games do not
work.
If you clicked Don't Allow: open System Settings, Privacy & Security,
Local Network, and switch Grubcraft on.

Hosting a game
--------------
1. In the main menu, click Play.
2. Under "Host a game", click Host.
3. The lobby shows your join code (like K7QD-M2XA) and your address.
   With Tailscale running it also shows a Tailscale code for friends
   who are not on your Wi-Fi.
   Friends on the same network will also see your game in their list.
4. When everyone is in, click Start.

Joining a game
--------------
1. In the main menu, click Play.
2. Under "Join a game", click your friend's game in the list.
   If it is not there, type their join code or address in the box.
3. Click Join.

Everyone must run the same version of Grubcraft. If the lobby says the
versions differ, get the same zip from the same person.

Playing over the internet: everyone installs Tailscale (tailscale.com)
and signs in. The host shares their computer with the others from the
Tailscale admin page (Share). Friends then type the lobby's Tailscale code
(or the host's address starting with 100.) into Join. The game list only
finds games on the same Wi-Fi.

If the game crashes or won't start
----------------------------------
Grubcraft keeps a log and writes a crash report. Send the newest files
from these folders to whoever gave you the game:
  ~/Library/Application Support/Grubcraft
    crashes   (one text file per crash)
    logs      (grubcraft.log is the latest run)
