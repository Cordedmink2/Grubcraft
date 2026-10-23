# assets-src manifest

Generated 2026-10-12 right after download. SHA-256 of every file, so a later re-download can be compared. Raw Quaternius files are **not** in the repo (see docs/16): their hashes are listed here, the files live in the project's shared `assets-src/` folder.

## Licence evidence (read at download time, 2026-10-12)

| Set | Licence line seen | Where |
|---|---|---|
| Kenney Castle Kit 2.0, Fantasy Town Kit, Nature Kit, Impact Sounds, RPG Audio, Interface Sounds | "License: (Creative Commons Zero, CC0)" in each pack's `License.txt` (copied next to the files) and "CC0 licensed" on the kenney.nl page | kenney.nl/assets/<pack> |
| Samurai, author null-painter-error | "CC-BY 4.0" on the OpenGameArt page | opengameart.org/content/3d-lowpoly-samurai-with-animations-fbx-format |
| Spider [Animated], author PantherOne | "CC-BY 3.0" | opengameart.org/content/spider-animated |
| Ant, author mujtaba-io | "CC0" | opengameart.org/content/ant-3d-model-rigging-animated-low-poly-ish |
| Alien Bug, author CDmir | "CC0" | opengameart.org/content/alien-bug-animated |
| Quaternius Ultimate Monsters, Ultimate Modular Men, Easy Enemy Pack, LowPoly RPG Characters | Pack pages say "License: CC0". The packs contain no licence file. quaternius.com/license.html describes a Quaternius Asset License that forbids redistributing raw assets, so they stay out of the repo | quaternius.com/packs/<pack>.html |
| ambientCG Ground054, Grass001, Rock035 (Rock035 also baked to 512px PNGs for the terrain shader, see the last section) | ambientCG API reports CC0 for all assets | ambientcg.com |
| Battle-cry voices, read 2026-10-16: Voice Clip Pack - Male Adventurer RPG (author field wolfwoot, uploaded by 2DPIXX; the page asks for credit as "Brandon Song", optional) | "License(s): CC0" and "Feel free to use for whatever type of project." on the OpenGameArt page | opengameart.org/content/voice-clip-pack-male-adventurer-rpg |
| Battlecry, author spookymodem | "License(s): CC-BY 3.0" on the OpenGameArt page ("A deep warcry before charging into battle."). **Credit required**: it is in CREDITS.md and on the credits screen | opengameart.org/content/battlecry |
| Zombie / Skeleton / Monster Voice Effects (author field ArcadeParty, uploaded by Snabisch) | "License(s): CC0" on the OpenGameArt page | opengameart.org/content/zombie-skeleton-monster-voice-effects |
| Ghost Monster Voice Moaning & Growling, author qubodup | "License(s): CC0" on the OpenGameArt page | opengameart.org/content/ghost-monster-voice-moaning-growling |
| Undead Moans, author AntumDeluge | "License(s): CC0" on the OpenGameArt page, "Licensing: Creative Commons Zero (CC0)" in its description, and a CC0 1.0 `LICENSE.txt` inside the zip (kept beside the files) | opengameart.org/content/undead-moans |
| 80 CC0 creature SFX (author field rubberduck, uploaded by Chasersgaming) | "License(s): CC0" on the OpenGameArt page | opengameart.org/content/80-cc0-creature-sfx |
| Insect or alien scream, author qubodup (uploaded by TheNess) | "License(s): CC0" on the OpenGameArt page | opengameart.org/content/insect-or-alien-scream |
| Chant (Gregorian chant), author christislord | "License(s): CC0" and "Copyright/Attribution Notice: Public Domain" on the OpenGameArt page | opengameart.org/content/chant |
| Bell dings/chimes, author PWL | "License(s): CC0" and "Copyright/Attribution Notice: Releasing as public domain." on the OpenGameArt page | opengameart.org/content/bell-dingschimes |

Not downloaded: Beetle Golem (CC-BY-SA 3.0, rejected), Poly Pizza models (site returns 403), KayKit, Poly Haven.

## In the repo (`assets-src/`)

```
323f1fd73d8a0008559738972c8b53d3b4785c86ee571b0fd6d237b19a8d8851  cc-by/samurai-null-painter-error/samurai.fbx
54302cf4ee2d2f2abaa8f5207c83f11b17c39ec6ca951d4a4362c31f9dbd07e4  cc-by/samurai-null-painter-error/samutex.png
df00b8879b12255651b852b3b4a257f4fca6228600760604ec141dbb2101be56  cc-by/spider-pantherone/Spider.blend
fcc3c91135c43627e0fc16e45f4b6bf6e5a897c4ce6b6e469ec2392989571f5a  cc0/alien-bug-cdmir/alien-bug.glb
4f389c5b373cb8b098b1f04a3e8bbfab4b914b3af886500d89eb8822a3cf3388  cc0/alien-bug-cdmir/bug.blend
029dcea9624b44b6a65a3b399a90517bc88d4d1275c258d6f6579a2f3310e679  cc0/alien-bug-cdmir/tex/bug-d.png
673f68c98d1de0ad9f8f3df7b0d9d49b3c64a34fbee82b32c38aefbd809812c5  cc0/alien-bug-cdmir/tex/bug-n.png
91854a6b6849bc72f57ac6d0f2769fa8b4b2c946638dba33f7b4f50016bc8ce3  cc0/alien-bug-cdmir/tex/bug-norm.png
b02f68ec9d005bbf63237abbdbda3fb0db243b83d75674b5eafacd9b78549b46  cc0/alien-bug-cdmir/tex/bug-rought.png
6f4de1451333964f8ffffe6ca9d15aaf6ef01fd089a8185b0268bd0d032fb662  cc0/ant-mujtaba-io/ant.blend
aac944f18106b3a3e29c6fdeec02523d4cab4c735abc01f5a8fa88a79ae173ef  kenney/castle-kit/License.txt
0dd97ff54f3875abe9d44c53732f2b870aa82e0dc40243104ba7eaa4196cedd0  kenney/castle-kit/Textures/variation-a.png
86c24e2d004ef945ecae6d89a811ab369588195b90c2f803939e996895f17896  kenney/castle-kit/Textures/variation-b.png
3e2177899d218e70793e63553e03e2a689a944acc056aefe3356a94be8979476  kenney/castle-kit/Textures/variation-c.png
048fac1067071c4e8111d0b3f843560e813ce22224d5da42ae5547d3e2d1265d  kenney/castle-kit/Textures/variation-d.png
94973a973e80e085185bf2f616e3529369a381636af9f0d22fd592c30740d743  kenney/castle-kit/Textures/variation-e.png
fe27cd1c3f7ff77b67839ef4a8fc8667d137a0f030413eead070a0bd73a24b48  kenney/castle-kit/Textures/variation-f.png
6b7628b53e9402d8da77c061dd5d9c7f60c95e4fee272774289d330504f68ea6  kenney/castle-kit/Textures/variation-g.png
66fd49be148f32e88f6c8cace67120250d1943a7856601158ad0ff24651db0b0  kenney/castle-kit/glb/Textures/colormap.png
9fbf9a309fed2063be177ef9518dd707787f8fa2b48d2eda8a52e59bb8829265  kenney/castle-kit/glb/bridge-draw.glb
c85521e13dcf55ba8d015a04bbdbd0f063c31d01befb69a7c9fb097d128f195b  kenney/castle-kit/glb/bridge-straight-pillar.glb
ef15989c3d5c7392563888bbedda86de3c1ec4b3013c71c61e25b5257a6f9180  kenney/castle-kit/glb/bridge-straight.glb
d9bc817e776e34fc24ad5b448b8cfc2c02603e79f6d90597de8936dc65a07359  kenney/castle-kit/glb/door.glb
e6a4433f5770b9fc31bf67d72424a877d5c010486d5b7373ce7af900566827e4  kenney/castle-kit/glb/flag-banner-long.glb
9561b6afd1d551199d02f520d31eab46dbff137eef4f9aa13f12e311b588fbd7  kenney/castle-kit/glb/flag-banner-short.glb
741368448bd21991700a488dfc5125330594c4dccd9395feecaa6bbfa682b65d  kenney/castle-kit/glb/flag-pennant.glb
74e94f48dffb40aca288676191265523c3e752aef40b868bfa3a51394083519b  kenney/castle-kit/glb/flag-wide.glb
263fc3c7bedcfcd2b30f1aca6feb76c8f249f8f79dc9a694f47ec63b77b180d6  kenney/castle-kit/glb/flag.glb
fdc3d5b07f619dcacbf2af36024dc9a356696e541f6ce409a5305be782665ec9  kenney/castle-kit/glb/gate.glb
2593ba3265f3ecdf2ee0a73580986ae6d9b76a536497356cdf093bb2735ecd6e  kenney/castle-kit/glb/ground-hills.glb
470e9637694bba581e7a67a07399920c9785d8fa9a01a688c80dfc6a0c06ff77  kenney/castle-kit/glb/ground.glb
7d41f469c9ed3000370bd7995b2fca5ac77ebe0ae1d1f321bb7a9d6f4b86f677  kenney/castle-kit/glb/metal-gate.glb
7ea1ee769eb063b192c3c3744cd32a11616320f7ca491fbe23289552f6655595  kenney/castle-kit/glb/rocks-large.glb
d860d3a053e1fc7b5825a39f6df68f14c49bab7443c1703e1f5b28444bd60e0d  kenney/castle-kit/glb/rocks-small.glb
a19ed023a15af7e148fcd0186cb73d0d26983e24de54e858655fa2d77799be46  kenney/castle-kit/glb/siege-ballista-demolished.glb
8b2317b6820dd9782c7c2b5da9b7cf77cc3704f3b4d27a4c32e50069852a6b0d  kenney/castle-kit/glb/siege-ballista.glb
e0663eb42a97c5d992b1864ba3459e8246de11cf29fd72f10ef5ffd271f3029a  kenney/castle-kit/glb/siege-catapult-demolished.glb
35a7484ff74630228e474fae885bdc9176c30e0af0d94def80307bdbc87b7583  kenney/castle-kit/glb/siege-catapult.glb
f895ff0152fb004c72310f6e7b51645babc8f8d22864c468b960f86252d2d8fc  kenney/castle-kit/glb/siege-ram-demolished.glb
6473c77747b1d0a2c273d5a013454a0c85e5fc33b831150dced46d89b8ff9a5e  kenney/castle-kit/glb/siege-ram.glb
6504a432e6e72a51c75f862ea08e144058727add6eb0f76dc7d9f7352e8a23d0  kenney/castle-kit/glb/siege-tower-demolished.glb
81909d6c33b181a9935fc7f800e8ee309454bbf99ff3b6b0ab2ecf16c3bf5e9b  kenney/castle-kit/glb/siege-tower.glb
6706dee3ec118323ea93ac58545cfcfd46e36c1ffffd750abbea54258aeda495  kenney/castle-kit/glb/siege-trebuchet-demolished.glb
123711fd475e35d061738f648d9bf46c7f463d57681325c369977cee8954679a  kenney/castle-kit/glb/siege-trebuchet.glb
22b52789ae3b0b208bcf5e9d5e8a70c6e307d51672217a122eaa58c42f2d7b36  kenney/castle-kit/glb/stairs-stone-square.glb
ec21b1d65c04ba6384734c31ac542bb4015541e652c411796fd7bcf292e6413c  kenney/castle-kit/glb/stairs-stone.glb
575e6463b9bc58b8294690b2c1a15409b0624e0253af1a215bf67ea5e9cbaaba  kenney/castle-kit/glb/tower-base.glb
1185110eb885e70652558465bfabd07b80bb09afaa7b4be0a2e309fd685982b9  kenney/castle-kit/glb/tower-hexagon-base.glb
131045c750296bc84e8f6192918270a4424217484b03627a73e9f584ac4f4888  kenney/castle-kit/glb/tower-hexagon-mid.glb
c2c0c2fe60bac1b0b27c65cfc118e98b2bcbaeeb9f32cdbe5b34b96a2c168132  kenney/castle-kit/glb/tower-hexagon-roof-secondary.glb
6bd3fb7fd8069419787e806990c9cf3869396e766a1b8cd16c2b3a1414961a0a  kenney/castle-kit/glb/tower-hexagon-roof.glb
66c7697375b61acdc42cf90c16aaf1340a285a93402426098bc2ab9175bb7bbd  kenney/castle-kit/glb/tower-hexagon-top-wood.glb
22a043cde795ebc6e5720daee4944f468de5f0ef62296495d0374d4b169ec892  kenney/castle-kit/glb/tower-hexagon-top.glb
f2dd82eca828d2b43c523927e6825539f5a648a18e7fcb68b7a4698f7d6d3d10  kenney/castle-kit/glb/tower-slant-roof.glb
90ccb41a792ea3c5b8c98a3f18b710b52ebfdcd1e389024168c5ac6e591dd564  kenney/castle-kit/glb/tower-square-arch.glb
7c48a3a8f8af838c0d7e114115c5abb7a557023b89e08aa421f14bd6add924e3  kenney/castle-kit/glb/tower-square-base-border.glb
0d9b180fba2229bf62deaaac381e16e912a0f5f2c48a8152e38fec2792e990b5  kenney/castle-kit/glb/tower-square-base-color.glb
996887e98957eeea2ba7b5d14f0d17877c91e4e99e1c5b8ee57d87b7f7c9d5e8  kenney/castle-kit/glb/tower-square-base.glb
a12235eb6e86d1e1b46a13e9ea3757ecb0e2fa8e5d82a9a0ae2017e78cf3baf6  kenney/castle-kit/glb/tower-square-mid-color.glb
1a7d065cb548b73adcdd497cac10397247bd70377cb2f6b9d36b78b61087b35d  kenney/castle-kit/glb/tower-square-mid-door.glb
aa95f69fd90b20c299628830a9abd635a48f440f25dbbe131fe3fdd003742c20  kenney/castle-kit/glb/tower-square-mid-open-simple.glb
dd5d242d41654cf4f60b1bbb05ab7263bc8dd9adc671feee08911bba7a46d569  kenney/castle-kit/glb/tower-square-mid-open.glb
68c186f978135ba77969cdbee502a1c0d9d8f40bd01e8aa33b7cc6859a2ba766  kenney/castle-kit/glb/tower-square-mid-windows.glb
34a43b162b0984df0c691cb12fcd169ff374412deca0de5fdfdd702a9fa0c98f  kenney/castle-kit/glb/tower-square-mid.glb
94215209b27f5274188d70906d2a4df357529abd878b6b916bfb61ac5e64d1c8  kenney/castle-kit/glb/tower-square-roof.glb
56b5677ad749ab114818638bf8a3fd8fcfb64ef25f627fd6d5aefe5203366698  kenney/castle-kit/glb/tower-square-top-color.glb
8e673350705e369a25afd6e8706946f0e41abeaf01b94abfe06c358e2e31876b  kenney/castle-kit/glb/tower-square-top-roof-high-windows.glb
b9345255366bc88744eea5469dddb4dedf2c70f33075b0fde53144ca251e25ea  kenney/castle-kit/glb/tower-square-top-roof-high.glb
2127ee2431efa89c4f40932093b43b9b724073a32fc7010547bd46bb58e4b04d  kenney/castle-kit/glb/tower-square-top-roof-rounded.glb
5516fb45c3158e03773e15e83f5fc8b9bafbbcb3f35fc8e1e06e823cdb01e3bb  kenney/castle-kit/glb/tower-square-top-roof.glb
5e98cdf2bf4d885edc3fa514719f80d63872e11c54ee629c11859223dad8489c  kenney/castle-kit/glb/tower-square-top.glb
ae2e4f11247fef4e8e550011c20bc1862839cd4cfd21590aec9781ae0fbbbc7f  kenney/castle-kit/glb/tower-square.glb
222537f0daa9167788a468b4ee71918d1066b7c142e7e251c449c50be3438068  kenney/castle-kit/glb/tower-top.glb
a72a7c237857a9fb04d51b05e3513e97a4d5d5572a709b3251f96b5bffd0a5ab  kenney/castle-kit/glb/tree-large.glb
50b4d452ee818afa9b37f325a594bdd4ffc1cef0f3c87ff3908251350c9960bf  kenney/castle-kit/glb/tree-log.glb
ad5b2d7225241eb0599dbc7f1a9f9c39915be56ccccf88ad091bc9c1b4334034  kenney/castle-kit/glb/tree-small.glb
f07a2ed87d6a636aba46a0ec1e9ffae27b17b4c6ae019e77924bdb9b8f6fe28f  kenney/castle-kit/glb/tree-trunk.glb
95a7e63c7716d31a47ffe1fe423c4ed65a610bd802b1d4286d743c1248255ffe  kenney/castle-kit/glb/wall-corner-half-tower.glb
2a64baa9c8adf4581c3ef28b7c4fc424890c9ed2da2d958bcbf38e239bb49f83  kenney/castle-kit/glb/wall-corner-half.glb
fd687db9c9fe6e3e446bd23ba2db23e993cf1119dd3fbcfbf543d23345c98d40  kenney/castle-kit/glb/wall-corner-slant.glb
82d27b869162b6a9ee19b0d7480913f56928b6620708f3b3899131495803cc9a  kenney/castle-kit/glb/wall-corner.glb
8daf5cccceace888d51d74b0e0254e192004faf926e07f1b28e94191879af3a6  kenney/castle-kit/glb/wall-doorway.glb
2241397b813c069858987750eaf7dce4a549fca239ae2e40b30fc7e58ff82b0e  kenney/castle-kit/glb/wall-half-modular.glb
4f0baca90eb52d96717aaabf36db98f1f2b554876f44da533b57dd92b6f67965  kenney/castle-kit/glb/wall-half.glb
3e0a2fb0c7d6b4d8bb52ee30c1a3f5bac01ad558dc382b15a5c5a2d52131aa8a  kenney/castle-kit/glb/wall-narrow-corner.glb
f01e3005f2cfe7451a0376a4fdba2408594cb60664e24e53e736e52d5b3ec934  kenney/castle-kit/glb/wall-narrow-gate.glb
d1791d278f01ad436c7062f9807e6280f11070c47cbb40567ea280c859fe3a72  kenney/castle-kit/glb/wall-narrow-stairs-rail.glb
51672d47bb3cec6f190334fcb29c3c2f464f605b85f5e13b918e868d72c632e7  kenney/castle-kit/glb/wall-narrow-stairs.glb
253d7f8fd3e9113ef5ae064eed49351b387532d83ccce284851d0a2190872fc6  kenney/castle-kit/glb/wall-narrow-wood-fence.glb
b536f63dd753575d551ace334549949bd3594559d62356f93b6a577f0efbeb4e  kenney/castle-kit/glb/wall-narrow-wood.glb
347fcdb0842ed47f20931b962dc0a2ad212690812386a3cb634016b4229145a7  kenney/castle-kit/glb/wall-narrow.glb
22d359abfaa3cf3dc5a2fece09a108024a07a4e263b21a4a0689da0e630699ed  kenney/castle-kit/glb/wall-pillar.glb
203abbac850f78bb2250005350427d60355532623173730a53e12674798e9637  kenney/castle-kit/glb/wall-stud.glb
bb99eb1d5d93affcb2466f74d69a370c32ce5b62307a081fc5a12850e741c707  kenney/castle-kit/glb/wall-to-narrow.glb
b2ea00acbf4ba91ff0d4b8b942edf9fa12d821abd0b2c945c57a2b9b3153cfe3  kenney/castle-kit/glb/wall.glb
fb8e4817197ef9f62215e95b4451a0f09c769c8e03e416e3a2ce108dfa6117e4  kenney/fantasy-town-kit/License.txt
d644fc35249e78f4436101d9e59795e42ecf4628a887e3cea5d2237317d51aff  kenney/fantasy-town-kit/Textures/variation-a.png
4aac939dc33195e35caf8c382ee3cb170054da763577f22d3c22692ec6afccdf  kenney/fantasy-town-kit/glb/Textures/colormap.png
16c43dfd257121fb93767e973b0fde09672502ca85383028c916e48126c049bc  kenney/fantasy-town-kit/glb/balcony-wall-fence.glb
3d504c3facb858d3110dcb2bba88af2c5be57d82bb07d52334a49195d022b84c  kenney/fantasy-town-kit/glb/balcony-wall.glb
5c9a1bb7d564270306fe438fd4a141fc6de0d102b355bbd1d2710249c489868a  kenney/fantasy-town-kit/glb/banner-green.glb
4e4a83de16e5979d2b34bb206d3bf3703a9039711bc658b37e3738d43090703d  kenney/fantasy-town-kit/glb/banner-red.glb
ef54ee87322191f18f7bf93d065add896fc584578c4c913eb99ea0bb18495a14  kenney/fantasy-town-kit/glb/blade.glb
4b0bce37e8645f3719a9032bf4d49048b7779ac891758ae6e2693238911c5eec  kenney/fantasy-town-kit/glb/cart-high.glb
63503cd0a7398310e01aabc8e3663f711a7517394d23d179d0695157fee99d40  kenney/fantasy-town-kit/glb/cart.glb
f7da0ca85be73a00f65aa297023e136e6bed93b7667ef49861a97ce89fd3a6e9  kenney/fantasy-town-kit/glb/chimney-base.glb
639eb7e9add8e491aa64aac94bd4ecbf1e67f8c628f3e0173f029f1c667b7590  kenney/fantasy-town-kit/glb/chimney-top.glb
a15cdbd13860092f6d793e5b785043eb90a49cdaa725bb71df9fd154ca68dac6  kenney/fantasy-town-kit/glb/chimney.glb
caa25191602fd94389e022c8e1f6a4963dd3b1457768e3ccff1dc8dbf4671aa4  kenney/fantasy-town-kit/glb/fence-broken.glb
be132586a950f4e4830815b909d9d53fb7e8653ff14c9f03c38c9fc550f4e797  kenney/fantasy-town-kit/glb/fence-curved.glb
cee58694ea5fdcca18e01318b355bfcd38eac6fad21b83f860039c287b150cc6  kenney/fantasy-town-kit/glb/fence-gate.glb
92261b9419108d84f3db0def5eadeffa783032029897562b1f512e4a7570ef48  kenney/fantasy-town-kit/glb/fence.glb
bd2337361b7ce93c73b69d2a8d6fbf4fe6bcffea6469f16d1643c960781fa2ba  kenney/fantasy-town-kit/glb/fountain-center.glb
76c96385cddbe153a75088d717f63624ac5e1dd948e84f6ae540140d23825ec2  kenney/fantasy-town-kit/glb/fountain-corner-inner-square.glb
d92a3d5aa8e5fd8dd3c4b1349171b549c57b6fe7e49e3c4776ad5d0106ec37ee  kenney/fantasy-town-kit/glb/fountain-corner-inner.glb
fb63467da36c0e14799ac2071791f9cb537eb2558206c81abf0e280753e19176  kenney/fantasy-town-kit/glb/fountain-corner.glb
aec3a6ebb4eda01f392054d3ee203d8cee74c9c45d584d3d9e5e2052949bca3e  kenney/fantasy-town-kit/glb/fountain-curved.glb
e75beda6d401c6c5f6a55ddc40f560a51f901156cc7ec17ad6a8150dfaec5650  kenney/fantasy-town-kit/glb/fountain-edge.glb
05cdd740e3175c2d3208d4cb6630a279a9b4cbc48393b28c92e3e40cca2a5d6b  kenney/fantasy-town-kit/glb/fountain-round-detail.glb
8d19741e2cca6cbdce114b950b4aa12af625979bbc17b2910ef2366834af5565  kenney/fantasy-town-kit/glb/fountain-round.glb
c201d8e39a8699c5551c543f5dae7779b1303cce71d7769661e66282cd628f63  kenney/fantasy-town-kit/glb/fountain-square-detail.glb
bc5613a8f1f514fac805f29f2f8f1ba3dd4cb64b449f51dd02cf565a3e5c9523  kenney/fantasy-town-kit/glb/fountain-square.glb
60e7a954c1dc308193f8330d624e59d8bd2f688d1d7c5b0c1da608eb5cd431c2  kenney/fantasy-town-kit/glb/hedge-curved.glb
688772aecf1c61410bfe0a257f51082bccaabed65d1794bad04226d444e7629e  kenney/fantasy-town-kit/glb/hedge-gate.glb
aee5d522711385fbc66f39c4f91e955a806b31128522ba2285c5d3374fbc9047  kenney/fantasy-town-kit/glb/hedge-large-curved.glb
22525eeeb1075a220d552085100620b8f0de6877bb903e3407c455d365d89a33  kenney/fantasy-town-kit/glb/hedge-large-gate.glb
f3aafd7064fcb53a1783cb04997d2f676038a55e0941bada74f39bcc7c86afd3  kenney/fantasy-town-kit/glb/hedge-large.glb
12001b57dbd3ccaddd217c49e541620137d3e835d873ce0bcedc0e01180a08a9  kenney/fantasy-town-kit/glb/hedge.glb
f270637e3083d6e84a30626ee05df2b09597a642985a7f5c3e4ab5bb1c7a8d55  kenney/fantasy-town-kit/glb/lantern.glb
515edba27d8c6326cee6f5912b7270224672f9f3dd61e43e85e1d504913ef19c  kenney/fantasy-town-kit/glb/overhang.glb
1eda742ccbc67ead6edead25c687f55d8671b88786b9e23c08ae7982bccb4b7c  kenney/fantasy-town-kit/glb/pillar-stone.glb
569c83dade6063399576640e80db35c34e422817ec132419fc2e2695189be5cf  kenney/fantasy-town-kit/glb/pillar-wood.glb
fe096b7ed7b29104724feab61d65ebc06966075955bf6e044eea635ef1076043  kenney/fantasy-town-kit/glb/planks-half.glb
20540c1d80f8b4af816e139a31f6a385f436dbf2d03314176ab99ca35452e11c  kenney/fantasy-town-kit/glb/planks-opening.glb
619705afa7ad290bfa499a701fd7ea07210ad535126cb3abc245b6f16e1420ab  kenney/fantasy-town-kit/glb/planks.glb
058d3816d57dca64dbc1e9a30e85c6e5e3aca2561bf57083acdce0ed3f375b29  kenney/fantasy-town-kit/glb/poles-horizontal.glb
5e6e9cbbcf29b042191781f830f928471c57f1dc7843ee33e2f7c1bff3edf522  kenney/fantasy-town-kit/glb/poles.glb
a4d8fbe9080849dccb45de7ac5f5ded2e17a3d893b8c07338e24193b28e1f03f  kenney/fantasy-town-kit/glb/road-bend.glb
0a998ac30f4be6f3a7d9f2e1f4e61edf35c5d9d99201b1389024921d45038661  kenney/fantasy-town-kit/glb/road-corner-inner.glb
96525a726a15410faf52bb4b813a985704fdbca37aec6d93808b8c18b9ec1e7d  kenney/fantasy-town-kit/glb/road-corner.glb
d3381ca4cae823dfa3f5f361810b6259197486e73c083793ac62ca312f182f6e  kenney/fantasy-town-kit/glb/road-curb-end.glb
236faeba46e3a0f5e3ec2b80f8ed57048e5b95a1a407526ebda35a3da143317c  kenney/fantasy-town-kit/glb/road-curb.glb
afbdfd9ef63c5c1dc9a99bb4f3ecbc94308300995b7958483014cd7f234484fc  kenney/fantasy-town-kit/glb/road-edge-slope.glb
fdefb02b17eddb4543379841a727c41a9ca0b800953a681974e63798f80ce410  kenney/fantasy-town-kit/glb/road-edge.glb
97d7e6c5aa5762de408c2053ceed88b4be77ac6d3ca79588d11ab3e1e0fa9094  kenney/fantasy-town-kit/glb/road-slope.glb
85630c89588bdc19285a7b5e894c1ddac69d7ce2b069966453800519e446f900  kenney/fantasy-town-kit/glb/road.glb
425bea0dd3c8d9198a067a2faad1df8860c5b9eb53560ce5c67347a79852b9d1  kenney/fantasy-town-kit/glb/rock-large.glb
2f2ca3e9970124b81bde77cec3b1a337410a5c76336ce5dec5760aff1b415316  kenney/fantasy-town-kit/glb/rock-small.glb
236082a8c9dec4cef61f00f7f77e4a2050d1083852af65d27dd3c16a53c54b64  kenney/fantasy-town-kit/glb/rock-wide.glb
2b2f61d1695b88acdef05f0263ed20fb3a4f8d5c6aee12d3f9e5cebe29e08d9c  kenney/fantasy-town-kit/glb/roof-corner-inner.glb
86285841d25d70921548bce543ecde08b66e1171c49ee1cc6f6ddf23d7aa7577  kenney/fantasy-town-kit/glb/roof-corner-round.glb
122d00bf873e5928eecf334785f1daf005c6f04033552258e97bf1cb1000b353  kenney/fantasy-town-kit/glb/roof-corner.glb
48bc045365398c92291f65ab80038670b14f85974aadedc3d17847254a0881b5  kenney/fantasy-town-kit/glb/roof-flat.glb
41ffddc760e9599428310188d96a44af80f23a243f9255748f418fd70123fdc2  kenney/fantasy-town-kit/glb/roof-gable-detail.glb
2ee635e8550180bb7840f7fdfff0b0835a34a425ffb903f19657cde65c261704  kenney/fantasy-town-kit/glb/roof-gable-end.glb
584761438f511db6c0c36a602b0cb7a84484acaebf94b96c722d6c8f188d57e9  kenney/fantasy-town-kit/glb/roof-gable-top.glb
4bb6bdbcadbb8ebeb69d720e29f6c2e57c99d98966baa41fd86914d3c13d6039  kenney/fantasy-town-kit/glb/roof-gable.glb
fa4736aa73d17b55b20b5cfcbfc83160c4a18a4e9563ede334d0c7d2327ed727  kenney/fantasy-town-kit/glb/roof-high-corner-round.glb
3ecd2f3fadaed67a2c713b272c7b8f73f60709752f8529bb34f9f92925123a4f  kenney/fantasy-town-kit/glb/roof-high-corner.glb
e1ab3e28f921d131c6f6ede216f6df1a0497ce1556fcfa94cacdce44b7ec31b3  kenney/fantasy-town-kit/glb/roof-high-cornerinner.glb
ff3cff6e3fa40a6f82271b82c425d732f32374e14c1c63f7e9c0db1d55d54cb2  kenney/fantasy-town-kit/glb/roof-high-flat.glb
d0b95f79cca6bc347d9479fa7a80dfc2b65b56f0f1ed00799893a21b0d9b5f68  kenney/fantasy-town-kit/glb/roof-high-gable-detail.glb
5f31970df848c6ffa95de3300f786bec2a331c63b1cb505ad893027ca67bf187  kenney/fantasy-town-kit/glb/roof-high-gable-end.glb
5e94a86388c83c330f2b7178e6fb96bdb6674528538e1c5b29d4acfca3de995a  kenney/fantasy-town-kit/glb/roof-high-gable-top.glb
21c54b2645ba33846f43410e6cef4d4f9239bc172bfb12f3227a6ec232977e2d  kenney/fantasy-town-kit/glb/roof-high-gable.glb
f4cf3fc08e5155015af7bd28dca8b2effe96fe3743276a5561119a3e7368cba4  kenney/fantasy-town-kit/glb/roof-high-left.glb
dd26c8d572f05a4725317de01415a17819b2d12cf01c8907bb2116b4ad8e85e9  kenney/fantasy-town-kit/glb/roof-high-point.glb
1c04bb5ecf61a660126cf28d64f2d06ddbf645a9e76598bb7af5828325968f79  kenney/fantasy-town-kit/glb/roof-high-right.glb
b5bea885f9dc0a2b1195748a1c4b9994e16256ab740b6cdc6783ebbcf8b96946  kenney/fantasy-town-kit/glb/roof-high-window.glb
7d39cd8fdcb8bd652d8a40f5f54e5e3d2c41bc1bb84691334f75bc411024cb12  kenney/fantasy-town-kit/glb/roof-high.glb
ff3fdc7d3572ee0f20746ae4d5b02a6ad26c185468b48fd0950852231cf81f15  kenney/fantasy-town-kit/glb/roof-left.glb
a7dfd157128e04105e976d7a47f772e95e4bd9f3d73b87a00b43861f406b94f8  kenney/fantasy-town-kit/glb/roof-point.glb
5f88d69d73a0beb5d74a69928adcb8aa6464c4ec3ad34b7c154c4ddcb17ad5f3  kenney/fantasy-town-kit/glb/roof-right.glb
9c95c71b7ea2a284f9ea37d0f684fe5aa654cdff14815969bc97d234014bbd53  kenney/fantasy-town-kit/glb/roof-window.glb
8f245dfc04b8e76b2833081462f7530c2aea9114a703161c1d9a4a975927347d  kenney/fantasy-town-kit/glb/roof.glb
4065d32494ad3ffca8d4d351f27f7927ff0c4c8f8deb99a2464210bcef58a208  kenney/fantasy-town-kit/glb/stairs-full-corner-inner.glb
61431fa509a2ea13c43c5cb532dae2c70a08f56267ba83fe2ad76fa2e53ddb68  kenney/fantasy-town-kit/glb/stairs-full-corner-outer.glb
d2ebf8c1ec86f75024d9f7931951d7e8fc4665349365b9e6886fadebbd108e9d  kenney/fantasy-town-kit/glb/stairs-full.glb
4fd7ab97b286ab1c57ae66ab62eb0325a0f9b25880eeb24767a42036146bb5d7  kenney/fantasy-town-kit/glb/stairs-stone-corner.glb
715e36185b27fcc37a84c345740584a10897a9148ba66050cda4a49eae4f7262  kenney/fantasy-town-kit/glb/stairs-stone-handrail.glb
78794e160a030dedc6b5b7a376d50b129f7052683b3cdae9904a4ad5dbe96f38  kenney/fantasy-town-kit/glb/stairs-stone-round.glb
ce92a069280c069d675f030a7258a50f5271cd8070e84339c120faa6c115e61d  kenney/fantasy-town-kit/glb/stairs-stone.glb
def3d44694a5393d3ddc1c002d09db90387dcef820286961ac506b0b9fc7009b  kenney/fantasy-town-kit/glb/stairs-wide-stone-handrail.glb
b28dae600de5c1759993dd363e18eebb40db6db544d94f7f56fb8829bc7f38be  kenney/fantasy-town-kit/glb/stairs-wide-stone.glb
474816e2464c0a4a2e0b1d15ac999b9aa4b4f9a47a3521342cd8fdb77599a872  kenney/fantasy-town-kit/glb/stairs-wide-wood-handrail.glb
33f06f1d5f926997e2709741884fd1f4bd86ce4cfc753a74257d2a9e3efffdb6  kenney/fantasy-town-kit/glb/stairs-wide-wood.glb
206895c4887b620590c9e31ce00aa57179c042f6b1756c0d03c21a17a0431bbf  kenney/fantasy-town-kit/glb/stairs-wood-handrail.glb
f6be9e5f77572d493dce374b68d231f116800ce388a3dcb0d646032162ccadd9  kenney/fantasy-town-kit/glb/stairs-wood.glb
5a4b379488241394916991c4b482ec4863c05bb548ca8751f5c3935f1f6e5b9c  kenney/fantasy-town-kit/glb/stall-bench.glb
2da2f7b31cde5d943d7c08c457bbb39797ef0a607921fde0b8455f338ce40205  kenney/fantasy-town-kit/glb/stall-green.glb
aa20a630f417032c46aaefbc779fd2b3355b451bf05d31acc36fd664146866b1  kenney/fantasy-town-kit/glb/stall-red.glb
50c6b512d810687f40e27c7a43b2864e5f7bc84f16ff7a4112cea4807676c336  kenney/fantasy-town-kit/glb/stall-stool.glb
f0a636fccb7d57c38f5acd58881c468f4872f1bcda1e9fd9b1118e76110565a1  kenney/fantasy-town-kit/glb/stall.glb
da4349701dd3fc474f7bb03cfe8b230bec0919025d8d1ecac0d1061a23c5e910  kenney/fantasy-town-kit/glb/tree-crooked.glb
a9af8768a5328c62694a42f67858a88eea974ad4540c5c3c54a7a70fdead7505  kenney/fantasy-town-kit/glb/tree-high-crooked.glb
7f372e338cbf5514a41e8d2afda81acd67277a4c5ad4e6e35a9acfa4a6e3bc4b  kenney/fantasy-town-kit/glb/tree-high-round.glb
2dbb718a81d9c3697c069495f58840f6c2002c8cdd13388be560c2199f6d03e7  kenney/fantasy-town-kit/glb/tree-high.glb
e8d81b97297de62699b92559cee7b247da84b1815df2515e42fd5a318a30b8d7  kenney/fantasy-town-kit/glb/tree.glb
24457e03e5dff3e074a79f9789953dd7fbb93bbd89254cdb751ddf4363a9a6de  kenney/fantasy-town-kit/glb/wall-arch-top-detail.glb
ae5f068bed777a7a698a770011b340e62b590751ebcc61d662b5b4a48369b698  kenney/fantasy-town-kit/glb/wall-arch-top.glb
18076d2770925a570c10708c2726e92f99226a940beb8435b21dfb71e20adcf9  kenney/fantasy-town-kit/glb/wall-arch.glb
c875a1d86faf9eab5af9152c0fd06b91cea4693e92780f949d505a5186925e32  kenney/fantasy-town-kit/glb/wall-block-half.glb
daa35e5b32d7941f3dc61f64f6f9630b70441478d652989cd6c02a908b84af86  kenney/fantasy-town-kit/glb/wall-block.glb
043f74b83254c32379d0fe60b066c5ded7f3396ccc91df443aea1763d492fa23  kenney/fantasy-town-kit/glb/wall-broken.glb
2382ce760563264c57e3e5fcccd9b31a80a97dc316b289813558d053ec22495b  kenney/fantasy-town-kit/glb/wall-corner-detail.glb
3e689a3e266865a44030d22fd645dd7605a1b6e7fb679bbb2355c18da6258344  kenney/fantasy-town-kit/glb/wall-corner-diagonal-half.glb
6218647612c1138f1897d4cde4774f399570a91411cca28d4095ee4d1e19f1e4  kenney/fantasy-town-kit/glb/wall-corner-diagonal.glb
dd8d24f82cd3d4d0ac9776a4d54aaefb69312d834bcfb0907bf8e63d1699ffda  kenney/fantasy-town-kit/glb/wall-corner-edge.glb
d67d4c6210d5e877ce24ccfb42cb3eab1576f7addb689924e7776d6b73e4165a  kenney/fantasy-town-kit/glb/wall-corner.glb
d55b3e931383c0f9f6311a586a01cca57d64e06146d260c8d08a9d39987f6d59  kenney/fantasy-town-kit/glb/wall-curved.glb
7af8fba9e7022c500289bee84fcbe02a1fdad0c2afc0276155a82c1021d6456d  kenney/fantasy-town-kit/glb/wall-detail-cross.glb
c568992d9f558b0f8778124c965770a7486d5ca7b371dc2fb3154cd6c0dfd528  kenney/fantasy-town-kit/glb/wall-detail-diagonal.glb
ff7607024a93d3e7256dc4975afb038e9299a08bdfb40a544fcabe40c9be29e4  kenney/fantasy-town-kit/glb/wall-detail-horizontal.glb
b1be46b9aae995aa34101e6aa62987624fbf9b3d814a67b4cb48d57958e38290  kenney/fantasy-town-kit/glb/wall-diagonal.glb
79b2d40db110ca96a6e77d4261da665fdf8c68f4c20c2afef5bce1c28ad72857  kenney/fantasy-town-kit/glb/wall-door.glb
4e2629d59e471e96c6661e29044cd32567191745671d00d75666328b2c7e1a05  kenney/fantasy-town-kit/glb/wall-doorway-base.glb
773ccde0f2b39d7fa61e5093fc4f11b137732a5becbe278c9dae03917895fe98  kenney/fantasy-town-kit/glb/wall-doorway-round.glb
d29b18fafb53b9280c7781b79514c5b3059a12c26586a78e0391eb055135b4a2  kenney/fantasy-town-kit/glb/wall-doorway-square-wide-curved.glb
74bd794d5b0b45902541da75555d60cc690359390d05f5cf364606347a0dd9ba  kenney/fantasy-town-kit/glb/wall-doorway-square-wide.glb
1ad6ca0f201c602d7ead56f395744ffd87ba8e5d6e16148d60d0c13e31cf85f3  kenney/fantasy-town-kit/glb/wall-doorway-square.glb
b64b5d8a8d3aa472162e3c00197fc24253a791a6100b8b63590d42eb2fb62eb4  kenney/fantasy-town-kit/glb/wall-half.glb
a09474e4d3992b5656e4999ceafa8c92500373b82c17f83216563c80dd6f6b54  kenney/fantasy-town-kit/glb/wall-rounded.glb
0ac077f8aa296060f548614e82a7fbfe6a09b5f8bad3453c7c6bb27e7df95119  kenney/fantasy-town-kit/glb/wall-side.glb
a7a1ea684f57370a796009353fb469b3f8eb676be544b7980e1b1059ddb5d2af  kenney/fantasy-town-kit/glb/wall-slope.glb
b0d666b1850d0b92734bdea736a062b4424fd909d13495f4a73719eb51bb41f4  kenney/fantasy-town-kit/glb/wall-window-glass.glb
ec911d8b3bd041ff46d5f5b21a04c2e1bcd006c51262866da94dc24a2c21e41c  kenney/fantasy-town-kit/glb/wall-window-round.glb
58a641a03fc4a5a225663969cc257fcf4f2fc63c199b4aca16b612142ff9eed0  kenney/fantasy-town-kit/glb/wall-window-shutters.glb
44516aac3d06ee8d2694cfd57d0c6e1e27ca2d0bc45b738ea1f0d1821da10110  kenney/fantasy-town-kit/glb/wall-window-small.glb
7ed37c9b795febc8b414d75ccdf9e8c04a94a791a9adbf6e548ed6ddbf0aba3c  kenney/fantasy-town-kit/glb/wall-window-stone.glb
6ad402b907878ba59bb148610eb50fa8439ac27b78eb3ede2e621f2d12235621  kenney/fantasy-town-kit/glb/wall-wood-arch-top-detail.glb
595a0235499eb7adc79c4bb8ddd49cda81e170d70632e22bfa83756909eae345  kenney/fantasy-town-kit/glb/wall-wood-arch-top.glb
bdfa9cfc79b0e7fd272d726acd8f087e97acbd2db7ec36348c2736b7bb7c046a  kenney/fantasy-town-kit/glb/wall-wood-arch.glb
8a31ac1fba82a195a02ef225f83e0bbe35208b604b4bd733142aa3c26810b490  kenney/fantasy-town-kit/glb/wall-wood-block-half.glb
c405053f4ac8ae0c85e9973613d205f0bc9dff0a5fe95a4910823384e131e824  kenney/fantasy-town-kit/glb/wall-wood-block.glb
53b61a341f0d6bd82256955f8dc29c4c714fcee115a9e803b28938fcfe1ae597  kenney/fantasy-town-kit/glb/wall-wood-broken.glb
712d57235e7d58e5b38b6766681834cb196c4efac5ffae5f0b5e5e270fdd91f6  kenney/fantasy-town-kit/glb/wall-wood-corner-diagonal-half.glb
43926de9d29254661bb5ee5cd939c3a32d495e763139c321d5c0a41fc362d801  kenney/fantasy-town-kit/glb/wall-wood-corner-diagonal.glb
f70b92efea5af19f4ac9c0c19af7727e8105f28a4c415313e8878fc4bde9d326  kenney/fantasy-town-kit/glb/wall-wood-corner-edge.glb
f4c38de352c80c110874fabe7323577d985af191c25242887574238146dc7d82  kenney/fantasy-town-kit/glb/wall-wood-corner.glb
2b7a46fea0256cb09262242a32e4842ca78a33d4e9feb89aeed0e7da3b67327f  kenney/fantasy-town-kit/glb/wall-wood-curved.glb
0335f9f55d541a2c044d11242103d5de70359ca5758e6734c8936ec27ab58043  kenney/fantasy-town-kit/glb/wall-wood-detail-cross.glb
3b1ef50674432944f3f9561e511c0867571393c58e3c73d475d383c5bfc469fc  kenney/fantasy-town-kit/glb/wall-wood-detail-diagonal.glb
ee27ac4f7b2a9dd944e1d3ee3e52d0018281033c768017afbc248b66e0d99764  kenney/fantasy-town-kit/glb/wall-wood-detail-horizontal.glb
760c3c9b8a3330a47c895e653e811ebf8be530a08a694d93514a601632a6c79e  kenney/fantasy-town-kit/glb/wall-wood-diagonal.glb
2aa4e982fcb6434be4e6051fcee7972799ff5350a3f36ca8bf71530b84608123  kenney/fantasy-town-kit/glb/wall-wood-door.glb
041c898ba4de75f1b1b163dd379b126d72abcd319ba5fa9b510521742543cb5e  kenney/fantasy-town-kit/glb/wall-wood-doorway-base.glb
5dcc990fa07242d74dc4382ed3f95a22f29777068c1c1795b22ef5d4448ac127  kenney/fantasy-town-kit/glb/wall-wood-doorway-round.glb
a246e8c19492085694a4874634b6027e12dfc8aea189872dee0b9085baf296b6  kenney/fantasy-town-kit/glb/wall-wood-doorway-square-wide-curved.glb
572c223df38d8b1e206446983dd9e3c92c944e7719c655a65880690a78ae0b14  kenney/fantasy-town-kit/glb/wall-wood-doorway-square-wide.glb
58b8f042cb1bafc282887d216340f406220b3407685f94e33c6a2f127dc68380  kenney/fantasy-town-kit/glb/wall-wood-doorway-square.glb
4b56875dcb488a0c17c01ebe8ce7337e891b3326860799b49f163f793cb882df  kenney/fantasy-town-kit/glb/wall-wood-half.glb
79fcfcfcf37716139ae76cc5fde7aea993a542751aa0df90a39cb569b2f319b8  kenney/fantasy-town-kit/glb/wall-wood-rounded.glb
2d0ae2509f596a929ae6ca24ff27ab96c0a2dbdd43847b02a2b8732106e9356c  kenney/fantasy-town-kit/glb/wall-wood-side.glb
56bdc7b8755fab80c753c1f6a7c856be15442fd89d0e53a1588ac2baba2401b8  kenney/fantasy-town-kit/glb/wall-wood-slope.glb
e0bb383a908d473181ad89ad7c676a0632f7ecd0f23e5f912216391d3e62bb69  kenney/fantasy-town-kit/glb/wall-wood-window-glass.glb
8bc2d9ce7246b1c9896ff2b285668f908755131faf2c2096b23b4a411f7e9aa9  kenney/fantasy-town-kit/glb/wall-wood-window-round.glb
c45a63690760cd5de3842ffbe222092e8cd483524679c4ba92fef4d1e52f7248  kenney/fantasy-town-kit/glb/wall-wood-window-shutters.glb
3c9118d167d8e75ee78f884b9edb90101f9c1928d4fcc2afaedcc349bf8c672b  kenney/fantasy-town-kit/glb/wall-wood-window-small.glb
d40eb3fe491c09e48fe17168d2445d60ca6d6b4ecc7c2f1caeafa22051000d98  kenney/fantasy-town-kit/glb/wall-wood-window-stone.glb
8699614fdbc99f565942709c0977c964e2c12f3b3c60098a6e2c397c973f08db  kenney/fantasy-town-kit/glb/wall-wood.glb
cab1dff37610e2a214237cc62ec0b1506e8fd76bb66c29f6c585b4b643b569c4  kenney/fantasy-town-kit/glb/wall.glb
3644d4092799f5ff672a5b07ff124f68259c8c9bd45d37f9659eea4426534b7d  kenney/fantasy-town-kit/glb/watermill-wide.glb
177e76c12f43845303638d1375c323675c3937b053b8762042fe19d3d7d27c5d  kenney/fantasy-town-kit/glb/watermill.glb
c049e479d0fa31d7cae7a3f4fe239172e7117789fdd10be4be5bddde6239d59f  kenney/fantasy-town-kit/glb/wheel.glb
72562df164f5fd36670521462da8f4161cf2c4c1c889799443f0fb44532ad5db  kenney/fantasy-town-kit/glb/windmill.glb
b9524658b52a99a20590f8feefc103088637fd13ea8ec35811aee785e1f2f330  kenney/impact-sounds/Audio/footstep_carpet_000.ogg
9af3c3f76705107339e72036e4053bdca3eaa50ad31c8c929f42ce8d40e5d781  kenney/impact-sounds/Audio/footstep_carpet_001.ogg
9af3c3f76705107339e72036e4053bdca3eaa50ad31c8c929f42ce8d40e5d781  kenney/impact-sounds/Audio/footstep_carpet_002.ogg
4ae7454338d57c7d102145919ae653c7d010eb542797c4f25a1c7470be3a7e4c  kenney/impact-sounds/Audio/footstep_carpet_003.ogg
4ae7454338d57c7d102145919ae653c7d010eb542797c4f25a1c7470be3a7e4c  kenney/impact-sounds/Audio/footstep_carpet_004.ogg
d7267e183067757c92c169de2a379abea592cfca6b39bb2e8feea15221ad79fe  kenney/impact-sounds/Audio/footstep_concrete_000.ogg
507a75eb19b897087855a3be3cf11a12d7fa37d143ad535eeebe0b8031c7325d  kenney/impact-sounds/Audio/footstep_concrete_001.ogg
b21fa546d74941196b25da8a20b7010abd8c05c4fcf93545d1f344e66a664e13  kenney/impact-sounds/Audio/footstep_concrete_002.ogg
8b513c2f0316324060b9174b8f28df6c2147418c506e6a53b77f18e149a5547b  kenney/impact-sounds/Audio/footstep_concrete_003.ogg
24dd8db2413e5b81ad181e1205c12a1c7801da5f85715d471b92a4b2f632e45e  kenney/impact-sounds/Audio/footstep_concrete_004.ogg
9d49497777405d78d7cf7f2888e28277f3a23192300cfd1d79d54876b20f479f  kenney/impact-sounds/Audio/footstep_grass_000.ogg
fb28781bd22b2eefdc14077f1adf5c94d7fe9c3a130194633b55f3e0d39d9f04  kenney/impact-sounds/Audio/footstep_grass_001.ogg
73a520139f5be716a403bfe9c80e0e94d1e61d8e9de04a354b336392164b53dc  kenney/impact-sounds/Audio/footstep_grass_002.ogg
0546e1d8ab2def714425701a18bd34c5ee2794b12e858e5b595aabcf38a34137  kenney/impact-sounds/Audio/footstep_grass_003.ogg
086fd4ab3546a507ea5f5c759a9a10e535c95ade13705c238bc0eeffd7d4e8ba  kenney/impact-sounds/Audio/footstep_grass_004.ogg
c05fa4a9dc9a7aabc04996466fb589537e49af76de9998d8f9e6270ec92dcef7  kenney/impact-sounds/Audio/footstep_snow_000.ogg
980fb9a1c9227aed7995067b0357f373053337f2416316db8ea0438418542fb4  kenney/impact-sounds/Audio/footstep_snow_001.ogg
db52bafdc66304f3b999f2c29227826c65993916e630ac2ffbca17d07b263178  kenney/impact-sounds/Audio/footstep_snow_002.ogg
68272e3f1cabcabd02d328fd344454d7c04cb58b5a86972fcdd858f845e3d007  kenney/impact-sounds/Audio/footstep_snow_003.ogg
7fe9ce5f5732aaa0a50ef02bb1ac162c27f6bd85214e86aaea6e13e7a41dbc00  kenney/impact-sounds/Audio/footstep_snow_004.ogg
5f1c252942a220d658121bd9417448cd19649ded9fff2ff15f6e50899373ce8e  kenney/impact-sounds/Audio/footstep_wood_000.ogg
e4f68e1c9fd55b4dc8ca89ed2bbab6f200171bc54bdc18aa96b80d8e7336f462  kenney/impact-sounds/Audio/footstep_wood_001.ogg
9ff095b05cee5eec40b081defe3f0eef7232bb16d4865ce03b9f6dbab875cfdd  kenney/impact-sounds/Audio/footstep_wood_002.ogg
2a68a083daa9eda08fed0ea5da5280dfbe0de226196c576be02f95259dd139d9  kenney/impact-sounds/Audio/footstep_wood_003.ogg
b33e0854e2b3f6b6492e25c25ef8f90447a332f7c6bb25dd02edced9581d3441  kenney/impact-sounds/Audio/footstep_wood_004.ogg
94b8bb5f2d43ab65e4bcc32b28562416e9bc2c51d9fd4be1e333660ee52f977f  kenney/impact-sounds/Audio/impactBell_heavy_000.ogg
9df61e3ae9a83dc65e5a1fd3ed19d480876f3b22b963a1b9ef6fa293592dcec4  kenney/impact-sounds/Audio/impactBell_heavy_001.ogg
a4171ed1a4a17fb858a38de1c59b5b42e50d8f046c525706c0e3ac8671f189f5  kenney/impact-sounds/Audio/impactBell_heavy_002.ogg
5572ccdeb3b62373b86fa955355b3a222c36e815f0636ba16165189a21cf2514  kenney/impact-sounds/Audio/impactBell_heavy_003.ogg
49fe4fafa2001bd0d312976796824571ca8429851f285997e1327d17bb34fd00  kenney/impact-sounds/Audio/impactBell_heavy_004.ogg
f0e982611e97512fee5f777986b67e8b435434b601f94992ec044f7e89fb5acb  kenney/impact-sounds/Audio/impactGeneric_light_000.ogg
c3cd1c073d186ae8fa35788ba94de581f1826e7427a7bb26490b2695fac18efa  kenney/impact-sounds/Audio/impactGeneric_light_001.ogg
d0bf60905b59ff630ffe01eb0edeeacd2504eaa8fb6523aeef7d2458b78264d3  kenney/impact-sounds/Audio/impactGeneric_light_002.ogg
0e8896706130f6bc88cdf1f0157aaacdbe88dce86cf0330f9136e44eb1209e81  kenney/impact-sounds/Audio/impactGeneric_light_003.ogg
f906fa3a37acc4372787b496efcedefa2856045d5ec9456a3bd6c303c0eabb41  kenney/impact-sounds/Audio/impactGeneric_light_004.ogg
b44b39a940e8948e74b9bd3776bff980df43cf220abdaf0d467dc4c43c0244a5  kenney/impact-sounds/Audio/impactGlass_heavy_000.ogg
58686e0e562612cc566791844a10e269f8d1921f4766c14ba43976fd0a429f4e  kenney/impact-sounds/Audio/impactGlass_heavy_001.ogg
4978654ac840ad519692d5d5ba48a8101cabce13b9a577be73ad589185e5e9ed  kenney/impact-sounds/Audio/impactGlass_heavy_002.ogg
aed07b154f24410c17e5262bbef832842fb275d41fe297f721af7501d0e9c23a  kenney/impact-sounds/Audio/impactGlass_heavy_003.ogg
c7742b0e33c1733ddaf902aa68464a4f5364e4d4cb6ff11b76a7aa4151a138dd  kenney/impact-sounds/Audio/impactGlass_heavy_004.ogg
d750cf88097d0c3e69920e6e14496e1444e1a9da01aa4cca8f5ec8cb44c57b4f  kenney/impact-sounds/Audio/impactGlass_light_000.ogg
05e513fbd42829060eb37855f8c3866722ec5c39a8937c9bfde518cc609effa2  kenney/impact-sounds/Audio/impactGlass_light_001.ogg
710db7451bb7857b9140dfe6afe4621ea2188ad65eda3919c569d1bb285b933d  kenney/impact-sounds/Audio/impactGlass_light_002.ogg
765290ce32fa108c939a0dbd6fbacbcd05abde622b57b708eb8a64058ef4225a  kenney/impact-sounds/Audio/impactGlass_light_003.ogg
f535b7cb7835400d8d28bcf9e1cf047ce3c0eb7a6456bdb1a4a27acbec1ed2cb  kenney/impact-sounds/Audio/impactGlass_light_004.ogg
9252d50bfb85edb17d6073c4a7806e10cdb9de56d3dbfc93a4b9727146d2df6d  kenney/impact-sounds/Audio/impactGlass_medium_000.ogg
d3d806cb3012c9ddd87010b30295ccb74d6d8b690077d19e67afd09a9d8feb03  kenney/impact-sounds/Audio/impactGlass_medium_001.ogg
48616f22c95589ed9fe55babf0fd012721d7b2ef1b9e85c2ad6408a2cd7c164f  kenney/impact-sounds/Audio/impactGlass_medium_002.ogg
dccea1ecc3d942ee5fd217c684632638f6c039c97cd3ae59c75ab15d0c3f0b59  kenney/impact-sounds/Audio/impactGlass_medium_003.ogg
853fe77ad46bd0cf106e40e741b039e15b90f73edecd92cde524fbad9e3ebe00  kenney/impact-sounds/Audio/impactGlass_medium_004.ogg
e07045693e4a2b3d165c424e3dab4c781d9ff8880a386880ac89a51315d7f831  kenney/impact-sounds/Audio/impactMetal_heavy_000.ogg
83554049f81f4db9209379e103c30bfa63f65c42189a03f300b045c2c82e23ae  kenney/impact-sounds/Audio/impactMetal_heavy_001.ogg
b914c8f1eb7c0f34bb165d7c77f4be0351f6be0660c13c53e65424e262e2c093  kenney/impact-sounds/Audio/impactMetal_heavy_002.ogg
b0f2ba4dabde9a87eb9c188a19d31e0c2300fd321adeba08d3b9b8aa011d7037  kenney/impact-sounds/Audio/impactMetal_heavy_003.ogg
6d65b463c0555dd5be16b8db6d2cbe23a94e07a4637779b8ad17d0db3e500a87  kenney/impact-sounds/Audio/impactMetal_heavy_004.ogg
33b5e6e37c6e9d54e07bf5a89b12c76e879f40c1ea83cdd82714df1d6f9fec6d  kenney/impact-sounds/Audio/impactMetal_light_000.ogg
fba69c467ddd85da7b1d82e259f936b876d9ba6a3d6f1b24bdd10468e46bfe08  kenney/impact-sounds/Audio/impactMetal_light_001.ogg
25a96f90a9a1f88a531e824e126f0519504625e5635e65a72e4f31611428db29  kenney/impact-sounds/Audio/impactMetal_light_002.ogg
92d5db6bfc672d9dc1b4f390b2900504f013187fa36335472554fafefa9050b1  kenney/impact-sounds/Audio/impactMetal_light_003.ogg
9b1c35820fba507081261f19ab5f461f69a5e2627a7d91acdb73ca3cfc997979  kenney/impact-sounds/Audio/impactMetal_light_004.ogg
a96f879fec0864a8938e0c745b6996a6c5679c16a234ce31a01cc995e8401003  kenney/impact-sounds/Audio/impactMetal_medium_000.ogg
e8a9eaba7c4d27422e4eeb3e6c7100d5d7dc0f83e005efc98c960adcc5265337  kenney/impact-sounds/Audio/impactMetal_medium_001.ogg
7e89ce2ca0dbda95ea2b78d4b50791cab35c10d20e9f5ccd45dd0b00e99ff548  kenney/impact-sounds/Audio/impactMetal_medium_002.ogg
2a185e2e57211d02c1bdd33b872d6b6a2fe83e6c14f4e56f426a95e717acf08e  kenney/impact-sounds/Audio/impactMetal_medium_003.ogg
03e10f800199c7166208e41c11cf4acfac6cc265467f02357da71972bc967295  kenney/impact-sounds/Audio/impactMetal_medium_004.ogg
36b4ea107222d073c67ca64dde26944975609202d5090b2f2021213c1a7e35cd  kenney/impact-sounds/Audio/impactMining_000.ogg
61235a18de3f3d92118235398222dbf51d448f1be70b1c8bc962b10b86f2d9c6  kenney/impact-sounds/Audio/impactMining_001.ogg
059558ec751115ca6b1441e00f0adc837c1a3eeb0a066b109e0fd879fc5d0b0c  kenney/impact-sounds/Audio/impactMining_002.ogg
4237fa2cd80364ad81cd0af44f67d7497f1c5edcc7bdb02cd22be2ba4580c83d  kenney/impact-sounds/Audio/impactMining_003.ogg
8ca3a99d85d845751ad8ce7b40509da58263a62e48cde70e5a30b5ccef4c9a3b  kenney/impact-sounds/Audio/impactMining_004.ogg
8403cd7a043f0391574c88c55dd205743725b5363d2407f6de145f201690b68f  kenney/impact-sounds/Audio/impactPlank_medium_000.ogg
d7c304319f8beac616231b62041fa8da74045b62d7155965516efb9b19303df4  kenney/impact-sounds/Audio/impactPlank_medium_001.ogg
f6a14da45994229676b4f29e6a35f504b5c2ec2b8166fb1a3b1aa004bc6911ef  kenney/impact-sounds/Audio/impactPlank_medium_002.ogg
0602a7b3eab5f53484f10fe775ee4204e7a2ba9703c886b302e012f7f6fab8de  kenney/impact-sounds/Audio/impactPlank_medium_003.ogg
6a51318c1c63821ea757622b57fafbf44c16c28129f6d39406a784260d87c447  kenney/impact-sounds/Audio/impactPlank_medium_004.ogg
112d4f93ddcc370b410630f971c0f5d991856102da9c76bc5c5540d388e75aaa  kenney/impact-sounds/Audio/impactPlate_heavy_000.ogg
142fd6c77f13d25f318265901163f70f8dfe12f87829169a570025be43de7011  kenney/impact-sounds/Audio/impactPlate_heavy_001.ogg
b0cab75d1befa32b4cf215d8ee7e1c400a39182e8b105963b8e310f825b198a1  kenney/impact-sounds/Audio/impactPlate_heavy_002.ogg
59775f8979a396c8a793b0a1db206d5bdc651eae808d858e9acaee8ebda5bc6d  kenney/impact-sounds/Audio/impactPlate_heavy_003.ogg
e31bfec90630155bcc6af541d1d14ae8202c435c52b42e6b6183fdb6ffb248b0  kenney/impact-sounds/Audio/impactPlate_heavy_004.ogg
6e797cf8a5bfc52d7ed5eb47c227974c1cf2a674954fec5b539cea7d62c7282c  kenney/impact-sounds/Audio/impactPlate_light_000.ogg
947915c2eb6ac57ee08fd3a48b297fd108d69abc72076e79882c21cad0eef56e  kenney/impact-sounds/Audio/impactPlate_light_001.ogg
8ac5869ea434cf0d364872774efe70b2d666a4f02ba35ddfc1e051d336721be8  kenney/impact-sounds/Audio/impactPlate_light_002.ogg
dbfb7c1d002576696df3e9a43d6639ee3f342068398b5e7b55bc2389d20508a9  kenney/impact-sounds/Audio/impactPlate_light_003.ogg
9cb33b0c0cce948313f078b60769d2f8e4075472494fc8f8129aa4a23c951897  kenney/impact-sounds/Audio/impactPlate_light_004.ogg
b670d30452983369771081319c268475edaaafe782bf4e8d70bd7eef09d8823c  kenney/impact-sounds/Audio/impactPlate_medium_000.ogg
68ba7701b71ef620f498f3038e832157d71ac8db7ad59424545890b250ceb24e  kenney/impact-sounds/Audio/impactPlate_medium_001.ogg
68762e1ed49bb59ae6602fad6ce2ebac61958478ddb2668de9106f010cd4b6b0  kenney/impact-sounds/Audio/impactPlate_medium_002.ogg
964c5fcfb91e7710e2af25c2243c8b7160756b223b0984bacc9eb25bfeea057b  kenney/impact-sounds/Audio/impactPlate_medium_003.ogg
f0e8164081190839caff6c2958361b22780ffba5617f77f2d4ee3a4275b318bf  kenney/impact-sounds/Audio/impactPlate_medium_004.ogg
b33a8f14068aec24ec69ba85e5e87fdc41228975f6a1a3e44a6e7d6fc3d9f8d8  kenney/impact-sounds/Audio/impactPunch_heavy_000.ogg
f92f5cb6ba4ff2766497292ffd90865654317eeca976f5652e0708dbdcdc0dd9  kenney/impact-sounds/Audio/impactPunch_heavy_001.ogg
7993dd4c156b9979ad69f17be5ebe31850b16039a3857f8477141be54dfee1b3  kenney/impact-sounds/Audio/impactPunch_heavy_002.ogg
4ab53b26f4ea575120dcc1348526c9e0db2afc6af7a442aa0489bdead2ca80b6  kenney/impact-sounds/Audio/impactPunch_heavy_003.ogg
f4c0c3eb8ab6517583b8218ed03f28923d1b687379aab4c1d5a6d4c10cf8e500  kenney/impact-sounds/Audio/impactPunch_heavy_004.ogg
486988aa2d6440ffc4c62a0e8ccf3c23673ba84424bd4723378d451b7255eb5c  kenney/impact-sounds/Audio/impactPunch_medium_000.ogg
e71d23abcc3f10d9d6bc9615b2a431fff70b12333a5fcdd9964457564ace4349  kenney/impact-sounds/Audio/impactPunch_medium_001.ogg
6492f9ccbc8d24dcef607bb1b48790b7870961474561f7fe5601e1c73a3018ef  kenney/impact-sounds/Audio/impactPunch_medium_002.ogg
2d7c719c05999e0532f4384e1018e04cd916db715140bf16dee2dff3f820e908  kenney/impact-sounds/Audio/impactPunch_medium_003.ogg
4a256878a3afb994ec9a4399336687748e68c7c136872c08c9c797d8c4e2464a  kenney/impact-sounds/Audio/impactPunch_medium_004.ogg
49e7ca88743fca974bb8676ea138b751cfd8f9033b5e7af8736c2a215d6edbc1  kenney/impact-sounds/Audio/impactSoft_heavy_000.ogg
4d0096364ba9e46119d2ff6df493fdc101bd2e1efae061da5e5f77a53b3fdcb6  kenney/impact-sounds/Audio/impactSoft_heavy_001.ogg
d130ae541951243d3f6ed963d0057450a445ffe117fbf9a084d34eb1635cb563  kenney/impact-sounds/Audio/impactSoft_heavy_002.ogg
d00286a2dc62ee5cb4d42bf56120e7050a855f69e655fc1294410aadd337eba3  kenney/impact-sounds/Audio/impactSoft_heavy_003.ogg
e008f0b6b14f62339c8fe9f9b45cbd6317ba4f66604c4899053b8017020a36f0  kenney/impact-sounds/Audio/impactSoft_heavy_004.ogg
7d3ba0bb5e60a11b5d3e558c141303dcf494256675fbf753c0d252d2cf0481e3  kenney/impact-sounds/Audio/impactSoft_medium_000.ogg
7642a4fd43e547afe4f7adfadb3dabb681c0ff512f52c1674bae30a726841faf  kenney/impact-sounds/Audio/impactSoft_medium_001.ogg
5069e3571a77d7f7aae9ef71d0364aa245fb7d64a7c8cc9956f221d03088c089  kenney/impact-sounds/Audio/impactSoft_medium_002.ogg
5c4a1f35fde7e14046931da7bc3d1b23736541b7190ba107e08a379c4ca43cd6  kenney/impact-sounds/Audio/impactSoft_medium_003.ogg
b0a4dea90eed47accf50b768beff1166f2f1486fb139e4d025b2427637624bca  kenney/impact-sounds/Audio/impactSoft_medium_004.ogg
f4bbee66ca191ec744b84fc07ac9ed2a6fda955d51cfed8360f12b62815d3cbd  kenney/impact-sounds/Audio/impactTin_medium_000.ogg
84253095543c094d83cd749f42ecfbc96e356b60385f18a192c18eafb25b3cc6  kenney/impact-sounds/Audio/impactTin_medium_001.ogg
c1d2c4f9910d4529581ae56a9a9a01454c4619fbb7d6da7616221d8ec58cc0c4  kenney/impact-sounds/Audio/impactTin_medium_002.ogg
0d18737d93ba7aa0669f751094cbc1671bbebac4fe207783922938ce75e24334  kenney/impact-sounds/Audio/impactTin_medium_003.ogg
fd53e24efec3d5c1cee5a46fee70f97efd4bd01e53ac5d9b5f3b6719dfe9f5aa  kenney/impact-sounds/Audio/impactTin_medium_004.ogg
15ff82332f342c30e469215539e1cc57e11b221aa2cf7e11e811d313a9927f3d  kenney/impact-sounds/Audio/impactWood_heavy_000.ogg
f1b07a53ac10c615fff9ba470511fdeb722b5902e1a8520759fc20ad6f546015  kenney/impact-sounds/Audio/impactWood_heavy_001.ogg
ce1e06f2a05f2634c645cb72f6a1623e64bc2d92be34917430a61031fb6597d4  kenney/impact-sounds/Audio/impactWood_heavy_002.ogg
af07511902b309b3f6d8f0e1fafbe93869127fb88099ebdc0aa6a00a98dc3cb0  kenney/impact-sounds/Audio/impactWood_heavy_003.ogg
2c647ae6bd29c72ae2523f7e16e325d8d331b75cc99ab4f637c48a0e11c34677  kenney/impact-sounds/Audio/impactWood_heavy_004.ogg
eb45e35b4bed784d3f120de4afec4b0d6ac09d9e143cce8cfcb7d739e1b1f3c3  kenney/impact-sounds/Audio/impactWood_light_000.ogg
4b76bf3ccc8e60d19188f3165b778a7817786faa6887c55d9049bcbeef3b425f  kenney/impact-sounds/Audio/impactWood_light_001.ogg
b183cf7a5bd783817e0e4b82af34ac6658fa1927049089dadc8b15c2ecc771dc  kenney/impact-sounds/Audio/impactWood_light_002.ogg
cdfbe8af2fe7ff9ee28f337a0a989deee0f327620da456c8413644880e9d0004  kenney/impact-sounds/Audio/impactWood_light_003.ogg
e10009f67ddaeb3a59a3e1129cfc1615d84ba60ab578eae07e7092ac223f099c  kenney/impact-sounds/Audio/impactWood_light_004.ogg
1723a9fd25103ed41af06054814e84e65f4a772bac86ab2dc927da93d9592ff5  kenney/impact-sounds/Audio/impactWood_medium_000.ogg
15271dbe8e2872dbb0ff1aeb7b12b2d2b92b7c1951c88532b9ab9158c36dfc73  kenney/impact-sounds/Audio/impactWood_medium_001.ogg
3f3f1917428b828f733a963167833c707bf88296ff158e1559c57c5cb813d799  kenney/impact-sounds/Audio/impactWood_medium_002.ogg
635c37cfcd854b187d643d450ff0397798dcc29ae94da32192b814fae6bd854c  kenney/impact-sounds/Audio/impactWood_medium_003.ogg
7ba48f2f99c91c71be420841a91f1e69c0cbc4fe887fcd7213144d67bb87b13a  kenney/impact-sounds/Audio/impactWood_medium_004.ogg
b49aa9c56b04528b95913de13e506a0f7c5e807b9925db9bfef86af1f91120db  kenney/impact-sounds/License.txt
07db973f79f6ae0f2edc34561e7592e24d0577455919fb602cb8ecc0da991dcf  kenney/interface-sounds/Audio/back_001.ogg
61581c58194e3f19f531072edabbc344204c7e0a2887b8ededce4357bcf09195  kenney/interface-sounds/Audio/back_002.ogg
ff5e87de1230b1a5f1fa83e5a4ba9d7eb1c07154267ac34b510a17d4caa03cef  kenney/interface-sounds/Audio/back_003.ogg
c22e3c49f42c2a5ba6748e91283efe124f32822469c1258c3e5d7b3d7ab9361c  kenney/interface-sounds/Audio/back_004.ogg
d21d0f0b782445db579d11e2506b24cd1ac9d664ee33aeaf807761aa7b6fd710  kenney/interface-sounds/Audio/bong_001.ogg
ccfb7fa0cccdd9faec0eb16033c732b1e308d139d80f799161495d58f7adcdb9  kenney/interface-sounds/Audio/click_001.ogg
adcd1f4adc35f1b41bc1b5bbefeff7aa44f2f3f0d96d3199b544140c7c1e761c  kenney/interface-sounds/Audio/click_002.ogg
2fa929138bc3a0f432696588f66eac94b8f6d463905ec8808c6461ce4b054292  kenney/interface-sounds/Audio/click_003.ogg
09e2c1b643b71b9aff9932b4e02d78db116446b9a14b00a5b2a9b28d2668fd8e  kenney/interface-sounds/Audio/click_004.ogg
a3ee7ebb036bc3cdc218ae52bb27548c4f49bcf07505274f8b61b1af8236acc2  kenney/interface-sounds/Audio/click_005.ogg
44af8249b933e0fd35ec957a638bb1a0b01f85b53fbe9674bf77bd3ca3168ef4  kenney/interface-sounds/Audio/close_001.ogg
bc4279d2bb2bf86592473eab6a8eab55e51b54a8619315000dbd780a83b102d8  kenney/interface-sounds/Audio/close_002.ogg
39a1a7f3bc001766c1881841e9903c6b16f5a10b5666e0791d7f6444677d8b05  kenney/interface-sounds/Audio/close_003.ogg
41b41e01ac75f4bc5a9c8dc3b0de744fa7f877757463692783c12a8ae4bfae6b  kenney/interface-sounds/Audio/close_004.ogg
063564703b6094d70718a3e787a55cc9141611e4ecd6b6637f8828f79b4a8c3a  kenney/interface-sounds/Audio/confirmation_001.ogg
33b17a9a9a2397c62b285c52c33a907fdffb476909c99e42dde603f6a7a8b12c  kenney/interface-sounds/Audio/confirmation_002.ogg
3091bf0be0497f825769ee0733ca7bdc3bcd59bd6c6e8f2ba8f93d580ff38022  kenney/interface-sounds/Audio/confirmation_003.ogg
568967a3d9f8a8f6af54ea01729c4882284308f2a27d78c07ffd7ee0d6951661  kenney/interface-sounds/Audio/confirmation_004.ogg
692ee3a6f1821402ca649ff735bee9591dc4846c8098fc7d6a90fd15fa57c1af  kenney/interface-sounds/Audio/drop_001.ogg
4ac4d1cef7e936965cbf795852ca2020300b9e2ba7daa59f2bf4f1f7bf416218  kenney/interface-sounds/Audio/drop_002.ogg
43d086d50f90493a98cd2e09aa6dbf150d164f1578e1ec768d21cb185571ad87  kenney/interface-sounds/Audio/drop_003.ogg
10738707f9300f9b963ceba46d5da2ed4a221faf7d39484a643c2d3bc151c5f2  kenney/interface-sounds/Audio/drop_004.ogg
46e67425d16339772e8d328fb36a49426c9467418686e11beeb71ff84b0f6433  kenney/interface-sounds/Audio/error_001.ogg
76810ec365f41078fd0507101da5bb7b8730fe011dde43908329fdabd8575e79  kenney/interface-sounds/Audio/error_002.ogg
885b28175c7b511118bcffab95675b48bbac9005cf6514656ebdad979276b3ab  kenney/interface-sounds/Audio/error_003.ogg
0b574cea597d96507e782ae9764f88482ce49f46e931e57054bf7150047f2d69  kenney/interface-sounds/Audio/error_004.ogg
3f75b11df5b06d68d2c25fbf0e9c9d9e7dd9250562a4007ee34856b8a309a2ae  kenney/interface-sounds/Audio/error_005.ogg
a97513d7fd2b12b1210c423be943b4c1ab0e899b11db95855e9b74836e646ae0  kenney/interface-sounds/Audio/error_006.ogg
f1ee4c1ac8bfe1a82eacb54b26f49e8974bab93bcf10c01800bc10dbf730f559  kenney/interface-sounds/Audio/error_007.ogg
eba17ecb2a426bfd4a8a6acff5f8a86202b6424a77af0fd842e37809a1ab6d81  kenney/interface-sounds/Audio/error_008.ogg
183eebee20eb0a532b0b85104d139cbf2673eb66d95c3cc733ac9ea98362e7e8  kenney/interface-sounds/Audio/glass_001.ogg
08e972c73e53c91d6d310e107e80db60edce78ae47225889a028e628a6f3da49  kenney/interface-sounds/Audio/glass_002.ogg
9a6f16e3c2eff6fe5d017a4755a28306b14555728831cd00d70308e8f9689566  kenney/interface-sounds/Audio/glass_003.ogg
2379b58f953e3d86982aa8cddcae763022c6258e9820171bf905ce58b4e97766  kenney/interface-sounds/Audio/glass_004.ogg
1f6c88008d0d9fe449197512811785e1d4638daf408a96a4a02756530e019172  kenney/interface-sounds/Audio/glass_005.ogg
840500be48078f1210f1fd7a9ac4e5665ed9b6f4a2b8dce91a237152a65e94cc  kenney/interface-sounds/Audio/glass_006.ogg
da78c642bbd95da2c364ed0e991ebbfc5a4c0d281165d932b5819e0dd31a39d2  kenney/interface-sounds/Audio/glitch_001.ogg
1da06a2e63ce131f5e0316ca27c75fc0d672db7a8f9b986c70693f2bd992eabc  kenney/interface-sounds/Audio/glitch_002.ogg
d55e0c64aee9f1ab2ba9523d0e256ca76639c3ec2f230405e2ff25aff36020b3  kenney/interface-sounds/Audio/glitch_003.ogg
d0f4db4d12f88592bc328bd621d356d099eaa2dda37bab37a6cf17891c5026d1  kenney/interface-sounds/Audio/glitch_004.ogg
fd3e75e1de2f2fda90aceeacaeb0427ac8bddd3b07a6ce2a8f9ba923df3771f9  kenney/interface-sounds/Audio/maximize_001.ogg
cc51162cb4506158b953b121bf479aad070056533ade5c5cd2f980183127ceb4  kenney/interface-sounds/Audio/maximize_002.ogg
df16f25d45189f5d44d6a482ab20d5067350ca8525a9affa1f8ce599e31a20a5  kenney/interface-sounds/Audio/maximize_003.ogg
59c7a24b204be24d2a44a86d1e4d18c2cf51fa0446967a14b2590e8fb3afaf34  kenney/interface-sounds/Audio/maximize_004.ogg
0ee06fc3ee494723fc95b2224f9dc0af857f2903e7f94d5d33d8e67f454b3cf7  kenney/interface-sounds/Audio/maximize_005.ogg
f050b3bb77cb0b901bc8df43d2cc1872c353aebae10b4030cf8ae681726ad343  kenney/interface-sounds/Audio/maximize_006.ogg
3400931f3f3a1e9cf253efef9069bcdf053edd2234e4ee9bf88977c389943484  kenney/interface-sounds/Audio/maximize_007.ogg
57c074e600e193a303ce781c04c0ccc191c6030f9f106594fd94a65e55435c17  kenney/interface-sounds/Audio/maximize_008.ogg
b4d56386258b3cc1c8f6e5a22465f8cd65de3d4309e022664050f43151e466cd  kenney/interface-sounds/Audio/maximize_009.ogg
8481912bc7fa5828924a00ef15b6cd613b994956e0817bd3122065c9b2896d2d  kenney/interface-sounds/Audio/minimize_001.ogg
670e47846dc003fe29d39c8dfebb8635bfcacae3c0916392c0e8bff659493ad6  kenney/interface-sounds/Audio/minimize_002.ogg
400e1041499de85ec8b5529a35ff36787288175c4696cf2018ecff24a3a3fd29  kenney/interface-sounds/Audio/minimize_003.ogg
fcfe6fbb03a728e90b907f34d2ec7bf3ea1857397c3e47ee01990c1be059e4f6  kenney/interface-sounds/Audio/minimize_004.ogg
4e6792a98c75d3ff187c0c55561c38c569e02c64757ebebb98831cfe84e3e172  kenney/interface-sounds/Audio/minimize_005.ogg
39d7c47855b9642f43548b7b132c20e20a1490c7eb13d10008c32fcb24da19f8  kenney/interface-sounds/Audio/minimize_006.ogg
e951505044de0d5d87b72ce7f312ac3fbbdde025ec8c10db70f9d2651a2c8947  kenney/interface-sounds/Audio/minimize_007.ogg
bc14c17875436dcaaf809c25db5e61454dbc35612880f329ff37a95ef9b6a865  kenney/interface-sounds/Audio/minimize_008.ogg
885c20fc8b5b52226288794842a403821cd16e9800ec1d76be7a2f1655542eb8  kenney/interface-sounds/Audio/minimize_009.ogg
a27c6bb0df7da1e6af5dd5937593c98bc58b6e513f42fe6a3254cd6a6949c648  kenney/interface-sounds/Audio/open_001.ogg
24ff224fe2c09c6aed1b41249825382dc74cf324b887ff279d8494912ce2beee  kenney/interface-sounds/Audio/open_002.ogg
bcdca6b5c9c33aa15ca3fb18a1fa8d98a01b528b85be08c5a610e57c75fc022d  kenney/interface-sounds/Audio/open_003.ogg
4e02b8287bc4d9e7bf7a889cb9e48613c8eb22a310dfb0bf29c2bfd0a1e4bea5  kenney/interface-sounds/Audio/open_004.ogg
be97ec4893a02d6eccfb678daa76c83e34cb2583b834ec2593d2641def739fa4  kenney/interface-sounds/Audio/pluck_001.ogg
c977fe249ff42d1c93a552b33abc13a8399df3879fa510475426e5c4bbac1da9  kenney/interface-sounds/Audio/pluck_002.ogg
abb8f9e4eb2071491b0b15f84ee302386b1847609310c1b000ae174639a6cc5a  kenney/interface-sounds/Audio/question_001.ogg
46056f10c70dda860e93bd27b66f4d33c0b7154eff28def06471b50028efa5b3  kenney/interface-sounds/Audio/question_002.ogg
00bafc564e29c12e99da584864a750260e7110378251d63d3aa5b3d727836efe  kenney/interface-sounds/Audio/question_003.ogg
585ecb58b529dc49b5c5ce5ba93427bf73b7052bae55e49c43ba8f169c93ac99  kenney/interface-sounds/Audio/question_004.ogg
5c294d2e1a6b6d07ee2d75b17105a55ad3527e5482dd60652dc49c2ea4c70096  kenney/interface-sounds/Audio/scratch_001.ogg
2b07edafc1f3f49c4813f2b0ae9481c566c1f9962a22d5f5b79da7111e058e56  kenney/interface-sounds/Audio/scratch_002.ogg
2d80371b55ebe2e8b916b84093a8ae7717d09a80c6b617079ade5bd0dacba7b7  kenney/interface-sounds/Audio/scratch_003.ogg
26009c67ef90ee7a5a2bc458ef9e7f5a9da83da9397a830cff9e4a1813914687  kenney/interface-sounds/Audio/scratch_004.ogg
65742c15dca9853611e1c9e83bd04ac0d693f79896715cafa90bad39745d7a62  kenney/interface-sounds/Audio/scratch_005.ogg
320c6aafb2c7a503eae508dc443b72d7f7c0a317bfd16813157ed400cd04cf16  kenney/interface-sounds/Audio/scroll_001.ogg
6fbcda22bc45349c66f1ba4b56d6c211b12264e83667de4b9af1e4ffcef74be5  kenney/interface-sounds/Audio/scroll_002.ogg
c2fecaa6e37c73d7141d0bd303d1c2231428d97c19ce83d50236a56804818643  kenney/interface-sounds/Audio/scroll_003.ogg
431398a3c12ba3aa30ead97f3e2f9b05eb00978facf04b31bda138ab77a2f54c  kenney/interface-sounds/Audio/scroll_004.ogg
e39d059462225efa31f758fb2e01871cc96781972b5062873a24a55c88174e02  kenney/interface-sounds/Audio/scroll_005.ogg
aec0c31ea934a35936ae0d2ab8fac8123c93aa5647f935853a58dbaf90278b7a  kenney/interface-sounds/Audio/select_001.ogg
4cc6ac5341b9b2c86fef80c347de028e9ab9f2e037586ac7d9fa59a403a1c012  kenney/interface-sounds/Audio/select_002.ogg
9a612fd2f513d52a296906b41071c990f75defe56be3bee60d2d4e2423b51b8b  kenney/interface-sounds/Audio/select_003.ogg
eb6cb435f3f2ab14f85f46a2bdd9073ec511d0f1f8d8c890c6636045703a5fd4  kenney/interface-sounds/Audio/select_004.ogg
ad09146e4ea33b931b2f5dfb4051a4f1fe4a36f1a48c42c5e9269c292ae21214  kenney/interface-sounds/Audio/select_005.ogg
b44550fa8d3276c072ce7c3c1315d949f7a28a95bc22b3618ad4eea62eac8abe  kenney/interface-sounds/Audio/select_006.ogg
cd4390dd2b24f09016a5fa1be63334fa0f1859811f4cf29c02e8e2750f6a2cd0  kenney/interface-sounds/Audio/select_007.ogg
f703b6b1c62dae1c246a46d5c2442802588f68be2df4d17e0d65cf7e0dbfc826  kenney/interface-sounds/Audio/select_008.ogg
3b74efad87f1e69dbd1d05c5ad952ab90859b5a3d11da6ec5bd79b4f03b42afa  kenney/interface-sounds/Audio/switch_001.ogg
6fc395be2ad1f99ce318a5d28f98fb76dbeb676d83a0b28239d8c1fb90736180  kenney/interface-sounds/Audio/switch_002.ogg
ca12dd32103f401481d5aff56d5e9831174e99bb577139c09b07d046eb8b837e  kenney/interface-sounds/Audio/switch_003.ogg
541c19cc1ab712a9efc5765df602e7d0da09d9cafdd297ce1e19b092289fd62f  kenney/interface-sounds/Audio/switch_004.ogg
e94dfab6fd6558068ab3ea97e22e4b4f16b83ee440e6abf33ec4c4e9fbe8a70e  kenney/interface-sounds/Audio/switch_005.ogg
2676ddc4e341ee557b4d1dc8767ef8655540d66fb3ded6a1614080dbc4dc6f18  kenney/interface-sounds/Audio/switch_006.ogg
eeaf60c2abc7e1b715e2b032401bcbf6b3753b77f96b238f861a034a75515967  kenney/interface-sounds/Audio/switch_007.ogg
9d057b918024c787bbe6c00beed13a7ef72a3fa0c259add3cef36582792f4230  kenney/interface-sounds/Audio/tick_001.ogg
869442f54214be902ec7437f79e400dc91c1bfd90d18a6dd2d5b7e41ffbf457b  kenney/interface-sounds/Audio/tick_002.ogg
0253d5f7d8d2afbbc370eac2a4539bb844239f0c98343e516d376b137c06d27c  kenney/interface-sounds/Audio/tick_004.ogg
ca1d2dde5f0b286abac4f070e23edab8f30927c8a533665cdf2ac6492a415e49  kenney/interface-sounds/Audio/toggle_001.ogg
1f564f5520cfa0764028ce245ad97599d3ece8f0ab504554c403af92ac72b900  kenney/interface-sounds/Audio/toggle_002.ogg
7e69aba356d590826cce291bbd9dc6a85d7c53858b0e3712fcad30e1ad080b23  kenney/interface-sounds/Audio/toggle_003.ogg
d7bf2236fc95a0fe1e3b278259392bcb062644e2d4ec5f9c96f7c09a9928262f  kenney/interface-sounds/Audio/toggle_004.ogg
f7966c773bbed0eca6a9c75081c44a178b38eae112724dbb5fdfbd4192d118a9  kenney/interface-sounds/License.txt
2ef665bd8b65dbe669f4560be38a6d8f04a162e618b15a49e3e71b58e558aab0  kenney/rpg-audio/Audio/beltHandle1.ogg
f3b692d5c83263c25bce779805b212dbab515d18a603c695fc6a1e8650cf5366  kenney/rpg-audio/Audio/beltHandle2.ogg
81e976532565f4372abd14e83d2684195fa548d0a28d345de221e56052454f32  kenney/rpg-audio/Audio/bookClose.ogg
fa81ac2fedc8c641661b87e349630a36c9800e795e0c800e029214efdbe26a7d  kenney/rpg-audio/Audio/bookFlip1.ogg
3409042acbfaebc24c1a6327dc073e36f9250841712e96c5040da566a97df73c  kenney/rpg-audio/Audio/bookFlip2.ogg
c85db5dceb3f1df073e960630277eaa88a5afda0477c1ddd68dad707621767be  kenney/rpg-audio/Audio/bookFlip3.ogg
953390534377222bee89ac8cd9e60a58fdc037c71a4d7c18c43cd647c7f34ba8  kenney/rpg-audio/Audio/bookOpen.ogg
26f3ce60fbde85b678b0ad7f6624d80535fe540d88232ad1e38e47a96662d977  kenney/rpg-audio/Audio/bookPlace1.ogg
831874e8fd142d4d0a03ff009f1f83966338a7878c0f6f38c613ae5b80dd731e  kenney/rpg-audio/Audio/bookPlace2.ogg
b1afa711e1cabfe5b2c56e20e20e0f9d82142d067727447fa992ddc43dbdb06e  kenney/rpg-audio/Audio/bookPlace3.ogg
d00c2b3c9fff07e376145c8c8c45c90e5084ec192f6ce0387db233f7b86f1486  kenney/rpg-audio/Audio/chop.ogg
ddb93a3671233f95da0e0b10367f082f7eb42fa6caaddcf776410aa8833c747d  kenney/rpg-audio/Audio/cloth1.ogg
8a7451193c38bc05483aec25dd193e163a15d44cf7fff04ac7912573231b0201  kenney/rpg-audio/Audio/cloth2.ogg
a48632d17a4f1416541d261a32a31612e01267144c407fa3ecab3ff410f0085b  kenney/rpg-audio/Audio/cloth3.ogg
e7ab9a6c4466dea874196c61f59bf1da05cfe58748f42fadd695d441a154a99b  kenney/rpg-audio/Audio/cloth4.ogg
f29560a2aea95bbfeb643e1e8f56ec2b6f7e32f32ce4e140645a36db81de3253  kenney/rpg-audio/Audio/clothBelt.ogg
d545b1d9f20e6188140aaf551a96f1e82fe0d15f7ca066fac8b065e1231e0038  kenney/rpg-audio/Audio/clothBelt2.ogg
8a346186fd297254248cab8e8117060a52a5cf2a84f603153a762108550ea95e  kenney/rpg-audio/Audio/creak1.ogg
8a990afdc03aebb91d528f5385e2f95582dbfa8e2c12c71098ab01be9142294a  kenney/rpg-audio/Audio/creak2.ogg
f9638787f95004b9d2d06f8eff8e99d4e594c21b51947daf562856fbcec905d1  kenney/rpg-audio/Audio/creak3.ogg
834d29c60a8a8bfb50b158cdb6b7dfa8f02812a408a1ee9703d038dfab0b1aeb  kenney/rpg-audio/Audio/doorClose_1.ogg
4aa4fe79836c53cd6d0cb0b22ddc33b9901429463ae1bb75131f072683f63f21  kenney/rpg-audio/Audio/doorClose_2.ogg
d0a2540e5e1639bc2fbeef34e76f28e9f9cb8a151736a0ac4c13859994328d0e  kenney/rpg-audio/Audio/doorClose_3.ogg
fd21c0e7a9d0317375d2561590f0770dd3380ee35507d064862cb44d6f71595b  kenney/rpg-audio/Audio/doorClose_4.ogg
4ab93bab96522d8eb109ff96dc57cb6765deb02448fe14c10472084be5bb2a0b  kenney/rpg-audio/Audio/doorOpen_1.ogg
68962fb0458c9bac59dec3adefa9849703c7c89d34712de03dd0075d095e79e9  kenney/rpg-audio/Audio/doorOpen_2.ogg
276403e72c3b71c47bc24db3083970c23f8e5551ffa78f436f83483c56f3f0bb  kenney/rpg-audio/Audio/drawKnife1.ogg
d5df6a4130cbb016f97b4883769a418917b9629cb4c21717030b186e0c73281a  kenney/rpg-audio/Audio/drawKnife2.ogg
a11ae62fb1a628425769d11a9de394980ad8909c31f4c9a4316f226963e21caf  kenney/rpg-audio/Audio/drawKnife3.ogg
097e1d3b74949b0145fda0519d40b7e0773ab82ec4858727f95be830927e1a45  kenney/rpg-audio/Audio/dropLeather.ogg
6fe61ef1fc3bcf0e253bf2eb64759db6cb69e2fe452f4d88cc597ecf78a3d601  kenney/rpg-audio/Audio/footstep00.ogg
313472dba31fd0c855376069fa368bb5a198c27251cc8398ef464578b7047a4c  kenney/rpg-audio/Audio/footstep01.ogg
2d9575b7dead6e1217ca113991fc5229e9b6d998ea0726ebbb277e7398ab888f  kenney/rpg-audio/Audio/footstep02.ogg
a86756b9af9f370455ab47844239bb07fd1bee7a113068c91f0fcd9b72703fae  kenney/rpg-audio/Audio/footstep03.ogg
fd264d72a7cd622b82abde202f4ed09fa7de95d34c36473ee938ce66cdf39830  kenney/rpg-audio/Audio/footstep04.ogg
817957e1f95d97fd36870064c41673895575cde17bccb245f50364c86ed10198  kenney/rpg-audio/Audio/footstep05.ogg
6cabfe7ae95d227a84066c6e17014ac31cc7c89b14ef51d7877e98352d4a64fb  kenney/rpg-audio/Audio/footstep06.ogg
42924cdbf998d5b3db180a339fd74afc94c570d594a536687e337ef1bdfdb346  kenney/rpg-audio/Audio/footstep07.ogg
668f14dc1254d0d0f33025e9c7dbcdf2c2f00462ed0e07fb8f099c1d711c1337  kenney/rpg-audio/Audio/footstep08.ogg
9324ebb9b2c5bcd078dcb523c6518d38a9f34081069d511602c2efb14ce06ce2  kenney/rpg-audio/Audio/footstep09.ogg
8a91f969e932df709df80ee124d86a51389eed9b67f22e5e716bc2bbf60d8dab  kenney/rpg-audio/Audio/handleCoins.ogg
4b857968d64f9ac9336a10ffac6694d7547e1f01566ad6857b30bc8db3ee6c32  kenney/rpg-audio/Audio/handleCoins2.ogg
ae3cbf695aa0a8b98b5a80a835be5a3ccbd5446a24db2235cd4c3cd6fc92ed96  kenney/rpg-audio/Audio/handleSmallLeather.ogg
a66f6db5918ad016c28a9f2768f127a7f39b5fa04481e5a3b4e78ebe2a1c282d  kenney/rpg-audio/Audio/handleSmallLeather2.ogg
4cd96dc630bed9840c15f1dd2306da2cc56a4da26a5d3f1a03c5a7265ac5e54f  kenney/rpg-audio/Audio/knifeSlice.ogg
6c2064d0ef988d1ec3d56868e823ea8823a5cac00f2742560052633529407def  kenney/rpg-audio/Audio/knifeSlice2.ogg
9851a69d0c613e13bceef08060ecc4148f098ef487927cbebe270d642398a3b3  kenney/rpg-audio/Audio/metalClick.ogg
ba9ba60b172b3ebc131a940f25793cd2e207aca7af73dc80d637277f060f1708  kenney/rpg-audio/Audio/metalLatch.ogg
159def979e8e386c2c539f5e99cc30a080eb2dcb6c911fa2e4ccc0785b2522fd  kenney/rpg-audio/Audio/metalPot1.ogg
8e1f820d217d380fde10e969d109db34303e3f4d11c3951926eddef813d04ae2  kenney/rpg-audio/Audio/metalPot2.ogg
d306e5b848f6843332d0ca19f8f7dfe5796aeb56d8273902b085df463273f1dc  kenney/rpg-audio/Audio/metalPot3.ogg
5735dfd72cb64cbbceda4ebc00c380c41ca680edb82ff153aa7c9ab97614c539  kenney/rpg-audio/License.txt
```

## Battle-cry voices (`assets-src/voices/`, downloaded 2026-10-16)

Real recordings behind the Ember Clans', Brood's, Shard Choir's and Carrion Concord's battle cries (`tools/gc_sfx/src/cries.rs`). Licence evidence is in the table at the top. Each download has its own directory; the files are the originals, unmodified, so only the ones the cries use are kept. A `wav/` folder beside them holds the mono 16-bit 44.1 kHz WAV that `gc_sfx` reads for each `.ogg` or `.flac` (made with `ffmpeg -i X -ac 1 -ar 44100 -c:a pcm_s16le`); the originals that are already WAV are read as they are. Downloaded files were treated as data and never run.

| Directory | Source page | Author | Licence | What it gives |
|---|---|---|---|---|
| `rpg-male-adventurer-wolfwoot/` | opengameart.org/content/voice-clip-pack-male-adventurer-rpg (`RPG Male Adventurer.zip`) | wolfwoot / 2DPIXX | CC0 | Ember Clans: shouts, kiai, victory and fear cries |
| `battlecry-spookymodem/` | opengameart.org/content/battlecry (`Battlecry.wav`) | spookymodem | **CC-BY 3.0** | Ember Clans: the deep warcry under the charge and victory cries |
| `zombie-skeleton-monster-voices-arcadeparty/` | opengameart.org/content/zombie-skeleton-monster-voice-effects | ArcadeParty / Snabisch | CC0 | Carrion Concord: zombie yells and death cries; a few human yells for the Ember Clans |
| `ghost-moans-qubodup/` | opengameart.org/content/ghost-monster-voice-moaning-growling (`qubodup-GhostMoans.zip`) | qubodup | CC0 | Carrion Concord: long hollow moans |
| `undead-moans-antumdeluge/` | opengameart.org/content/undead-moans | AntumDeluge | CC0 | Carrion Concord: the moans under every cry |
| `creature-sfx-80-rubberduck/` | opengameart.org/content/80-cc0-creature-sfx | rubberduck / Chasersgaming | CC0 | Brood: alien, bug, scream, roar and monster clips |
| `insect-or-alien-scream-qubodup/` | opengameart.org/content/insect-or-alien-scream | qubodup | CC0 | Brood: the long squeal |
| `gregorian-chant-christislord/` | opengameart.org/content/chant (`chant_0.ogg`, 163 s) | christislord | CC0 (public domain) | Shard Choir: the sung voices. Only two steady stretches are converted (`wav/chant-a.wav` 1.7 s to 3.8 s, `wav/chant-b.wav` 11.0 s to 12.3 s, both on 222 Hz) |
| `bell-dings-pwl/` | opengameart.org/content/bell-dingschimes | PWL | CC0 (public domain) | Shard Choir: the bells |

SHA-256 of the original downloads (the zips are not kept, to keep the repository lean; the files in them are listed below):

```
eb874f027758ccf9eca0449d09dc3220dc44eb4095e8b0da93351d276e176f7f  RPG Male Adventurer.zip
fe09126cfce9d90ad26936b116354d1c162de261a9053854a82696daee80e241  Battlecry.wav
041ef356efc5d947db903530d68eb55b7e6fbfb77b96fdd092ab81eb8be59d57  Voice Effects Zombie-Skeleton-Monster Human Male.zip
25962d146f4634a93efa7fbfe9cde63f4182189ced6105b4f92f23c65d8c2a43  qubodup-GhostMoans.zip
013d2327338585d950e3d55422ec0233b443ed888cdbf9908961502690bbe67e  undead-moans.zip
5d4002eea42f278e76543bb36e8ddd2cb978588a13271984191d8228f5d26768  80-CC0-creature-SFX.zip
274d956faa2be61bbdb8a8bedc8f992860dde7c405773a70880f478f83e400ff  insectoralien.flac
67631633064846d345aa7ed77a2a59509fecbab510281c106782cb5092330ba1  chant_0.ogg
8ce40f49cd211125303df59db0da4fe911e8b2f34d3847011b15137c6b5850de  bell_ding1.wav
e4a764a1e0a6284f5a5ec80caf3d44c76d3ff8db17ddab8b42abb6b143e83529  bell_ding2.wav
99b9cdb7b9536c97c6738f1e0762cc40a3e7ef81bc774b0b6e0322aa70c96434  bell_ding3.wav
b06e837aa5e5b626eba32ad87264b577164e2eab3b9180a955345d06197b6876  bell_ding4.wav
```

SHA-256 of every file in the repository under `voices/`:

```
fe09126cfce9d90ad26936b116354d1c162de261a9053854a82696daee80e241  voices/battlecry-spookymodem/Battlecry.wav
8ce40f49cd211125303df59db0da4fe911e8b2f34d3847011b15137c6b5850de  voices/bell-dings-pwl/bell_ding1.wav
e4a764a1e0a6284f5a5ec80caf3d44c76d3ff8db17ddab8b42abb6b143e83529  voices/bell-dings-pwl/bell_ding2.wav
99b9cdb7b9536c97c6738f1e0762cc40a3e7ef81bc774b0b6e0322aa70c96434  voices/bell-dings-pwl/bell_ding3.wav
b06e837aa5e5b626eba32ad87264b577164e2eab3b9180a955345d06197b6876  voices/bell-dings-pwl/bell_ding4.wav
a725a4aef74d75bf8ddc9bc90abd49d1ba8380c62d80ee1318cf97c48c7a4124  voices/creature-sfx-80-rubberduck/alien_01.ogg
a28c192579824352a0177d71d9f2fe32560d17b00fbc16297488bddd07ddcdc9  voices/creature-sfx-80-rubberduck/alien_02.ogg
8e417706c6ca3f7eb99d9500d17bf74ef977d16c540dfa9d695db4721cc30003  voices/creature-sfx-80-rubberduck/alien_03.ogg
0a46b3c9f1028ea251bc0aa82bb9bdaa2e6208ed48d5a4e83811be282bbe32c7  voices/creature-sfx-80-rubberduck/alien_04.ogg
7f6414bd745b244af6f24f7bd2d58b0d7cd1c670ea389e122a750bc588ce7134  voices/creature-sfx-80-rubberduck/alien_05.ogg
795a25119b46ec1b0ce8e059cb75ac48386fb6db581deb9d0bfd06dc27e6e6c9  voices/creature-sfx-80-rubberduck/alien_06.ogg
a7afae8bd3b448184e92bbb6162702e86322e5df58045715176c450ba4bbea3a  voices/creature-sfx-80-rubberduck/bug_01.ogg
7993707dcb082c4f9f297bec44f8aa276988d848f4cf10ea6046c1c45daee518  voices/creature-sfx-80-rubberduck/bug_02.ogg
947dad474b93a2786455b1772184f79f4881cf8bf212353638a42f662c808b7b  voices/creature-sfx-80-rubberduck/bug_03.ogg
478418f0d70667109145ca10d82e918c5eeb95c594be40186681179a883d64be  voices/creature-sfx-80-rubberduck/bug_04.ogg
1208729886f3f66e7d8018e8880dff073275fd37b30e1016dd5b164161482df1  voices/creature-sfx-80-rubberduck/monster_03.ogg
04b556b93bf3f99255ac362ebe7f905f750a8bcbb653f78ef7191adb91e71f49  voices/creature-sfx-80-rubberduck/monster_04.ogg
ceb532ee0fe775d6f86a821099b75f1790ab8022b963cb837c8c1fa4fffee5c2  voices/creature-sfx-80-rubberduck/roar_02.ogg
54da7bba4c4272b0b77dc3fe26a91d0a26d9280797fecb8197d716b47745eb85  voices/creature-sfx-80-rubberduck/scream_01.ogg
73aedfbcc7e5b3004507d5a79e95ae53954026036bf8d6f65aa0a4ca298dd372  voices/creature-sfx-80-rubberduck/scream_02.ogg
9bab240b1f8555a66f06ba1cc46f119d4493126b012870c1ea389ba8cde463dc  voices/creature-sfx-80-rubberduck/wav/alien_01.wav
97bd24e1522cbab93e6b11967701f56b61134f4075f9cf7a482fb0ad0c8b39eb  voices/creature-sfx-80-rubberduck/wav/alien_02.wav
3e8c3990ec5b082aa861d2c39da8e805c9675e2eb7ffb8a14efc3277040c3954  voices/creature-sfx-80-rubberduck/wav/alien_03.wav
ccb9a7c30d5d4af18c63a357834c6ebf7e925b216d0fd6f43845e72b9ce6df95  voices/creature-sfx-80-rubberduck/wav/alien_04.wav
93d15e37cbc4864bf76b7dff1052abe2ebd245e4deb2bd514ccd09113cbc3e00  voices/creature-sfx-80-rubberduck/wav/alien_05.wav
f940e52bd783818e4adb829184d4db9854deb5921434937611b6f2fbb219755c  voices/creature-sfx-80-rubberduck/wav/alien_06.wav
21096b4c9f9b7485f153a3d4111487531e2a0a691d0b8ad6f551dc262967375b  voices/creature-sfx-80-rubberduck/wav/bug_01.wav
85bf734cbc6d0267db878069087f1a44133b878c55f588c31b12fe013d17a985  voices/creature-sfx-80-rubberduck/wav/bug_02.wav
cad62c2531c7b559c3a155f9c9b3b043d38042cb5fe560171c19377489aa9c5e  voices/creature-sfx-80-rubberduck/wav/bug_03.wav
e4f1fc645adbd945b8f90474034cc6bfb58b244fb626f0fa89adf85599728dbd  voices/creature-sfx-80-rubberduck/wav/bug_04.wav
17636e1ff3588cd6a589faaa7453056d2996ec59e6df082181af42337814ce3d  voices/creature-sfx-80-rubberduck/wav/monster_03.wav
4609bc77db70a303abaf1797e5e2fdb0b923031594be6fbcb767ca00fa331a99  voices/creature-sfx-80-rubberduck/wav/monster_04.wav
558db5c462da8fe47838f045ff4417277e66a55c21471ecdf4ab04d89fcc5e3f  voices/creature-sfx-80-rubberduck/wav/roar_02.wav
46e0e163a4a25d97ae1846d8b278989b820a79ccb30ccf25015137d45b8b8fa7  voices/creature-sfx-80-rubberduck/wav/scream_01.wav
b9481789b1136f792c195286b4a821aabef56a5fc3e15f975d2fcf3b7fafe212  voices/creature-sfx-80-rubberduck/wav/scream_02.wav
6ecb024d4cdc5cc85743faa79d41cd68f3d3e75d8faf3f23191311eac708efd4  voices/ghost-moans-qubodup/qubodup-GhostMoan01.wav
fb987e07dffd1b546a37b587afa0995d55274caace3fcc7c692e532dafac4a18  voices/ghost-moans-qubodup/qubodup-GhostMoan05.wav
67631633064846d345aa7ed77a2a59509fecbab510281c106782cb5092330ba1  voices/gregorian-chant-christislord/chant_0.ogg
956f18dd112167fd02b251dc4981768777eb449ed1e06303f889ae9df8e739cc  voices/gregorian-chant-christislord/wav/chant-a.wav
94ffe5cb45f41a458f239c522870e203e7f1fcffd6ff850712c7cd2c39cf8535  voices/gregorian-chant-christislord/wav/chant-b.wav
274d956faa2be61bbdb8a8bedc8f992860dde7c405773a70880f478f83e400ff  voices/insect-or-alien-scream-qubodup/insectoralien.flac
f5c480f4c14587a2a3e5036e9d78308ae4f76280ed0d8b4986796fe385f90231  voices/insect-or-alien-scream-qubodup/wav/insectoralien.wav
18604485719a00414f368a3eedf074666ea7d0751bd4ee6f28f5510fcc3099a4  voices/rpg-male-adventurer-wolfwoot/attack1.wav
5a9544f0e44bbe82365340fda70d4b1ac3b63bb99c4f68bc67687233431c1786  voices/rpg-male-adventurer-wolfwoot/attack3.wav
4e67a283aba87cea4ef5a01b8c2baa353b522b0f2de66a3749a1a95123193a6a  voices/rpg-male-adventurer-wolfwoot/attack4.wav
1bae8f04a042152c656c915e8fb3635860cf1099527ebafc7199eb5c75f3f02a  voices/rpg-male-adventurer-wolfwoot/attack6.wav
e73f668a5b9c4bfa1ea97cbbd5342bd47d503c96c92a20119694b3f27d60f1a7  voices/rpg-male-adventurer-wolfwoot/attack8.wav
93254d118fdfb1dac17e9b76dc7f57ad847654818cd10ba5f8630c5e4a14026a  voices/rpg-male-adventurer-wolfwoot/attackbig0.wav
a8c74fc1f04f8619263c85cf8cc32ea264ba9ef5a7122650f4a90339f694bc6b  voices/rpg-male-adventurer-wolfwoot/attackbig1.wav
7fb7582565945efd5c8722bc6a7ed858086850c0fb93c58e9ca2836a08b52930  voices/rpg-male-adventurer-wolfwoot/attackbig2.wav
026d596cbafdd730e33422243010d63cc73088746e817bd89660201ca4a2506f  voices/rpg-male-adventurer-wolfwoot/attackbig3.wav
05679adb1522e4e656d9e66fc9b6bd343538fb2968edd37492733f2f8b1255c1  voices/rpg-male-adventurer-wolfwoot/attackbig4.wav
98c53fabbe6c7f584ee7164ed5c0407adbd4812b837c1c0ef90818025ff3ed7c  voices/rpg-male-adventurer-wolfwoot/attackbig5.wav
8f65c53268ed217346b0c403b0e4a2b3649af055c645f0f2d747248f58c5a162  voices/rpg-male-adventurer-wolfwoot/hurt2.wav
251f58f2e2d8ae8b5d2876fd3dea26848725b3b1fed18c196f1fceb5ed71993c  voices/rpg-male-adventurer-wolfwoot/hurt3.wav
78cd72438249a22e5833a67d852e4db3b5579066caa2b4bed1c0dfa887602ff5  voices/rpg-male-adventurer-wolfwoot/hurt6.wav
2777852ee5e29d3b95a5bcfefa2e51d156d43301460cb5d137b0369fb9df83f5  voices/rpg-male-adventurer-wolfwoot/hurt8.wav
e94fb7ded133d30b6757dcc09305394a6833d3febecde2c5fcb306adb5607588  voices/rpg-male-adventurer-wolfwoot/jump0.wav
3eec2046486166deb2cad4efbf59f2d3b71c5c0335354f659d7fd473d3fa81c2  voices/rpg-male-adventurer-wolfwoot/victory0.wav
ecdbdac1aaacc3e0b0ebbe44b6c20e4ac5e16498c430b8cd278471dd45d6d52d  voices/rpg-male-adventurer-wolfwoot/victory1.wav
cce4e7a43ab9bd5e34d58f568e21e724d8aee6d058d6e3d3372079e3fcc2c113  voices/rpg-male-adventurer-wolfwoot/yes0.wav
d6b8192091bfb95caf14cbd385b012263fe764656b0fd471cd2eb64b6afe3007  voices/rpg-male-adventurer-wolfwoot/yes1.wav
a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499  voices/undead-moans-antumdeluge/LICENSE.txt
6e2f45883c07b00345270e92bd21545dae7caec864045537c65e3eea093dd916  voices/undead-moans-antumdeluge/undead-1.ogg
c43c01f9264fa91539d8d78ed29f241ff20d4f498e6cf7a2e985fde2bdfe51ce  voices/undead-moans-antumdeluge/undead-2.ogg
73c60abc7641518dd871ba4abbdfd3e15f93207147ed3bd012e62eff97e35457  voices/undead-moans-antumdeluge/undead-3.ogg
bb18310d61c8c6d901cf71af35d6d10ec61186784532a688402384b5fa3006e5  voices/undead-moans-antumdeluge/undead-4.ogg
d43a9324282cd95ddd80948d35b7d704856c3ec6274f543fd1a123e9771d2fbf  voices/undead-moans-antumdeluge/wav/undead-1.wav
b26bdd4db3257b881d5a6f2bb8fb19ea213c3046d9f66d9665b0db3026ecd58b  voices/undead-moans-antumdeluge/wav/undead-2.wav
79f228b3b432a5b2d89ff74ed27c3ccf3bad001976cd749740372fa9f09ad63d  voices/undead-moans-antumdeluge/wav/undead-3.wav
8f3059e2143f0f55f7fb9d511e78e06e76ddb4bb9419b91c16e82dbd27740713  voices/undead-moans-antumdeluge/wav/undead-4.wav
a9951034cb829218d5051d3b753a3b43669ce71d9b8ca2d52445e25f76ddcf7e  voices/zombie-skeleton-monster-voices-arcadeparty/humanDeath1.wav
87d408cdff8270ecb0eb8226d09d45517f33f15654a03861073050e5f79a0ffe  voices/zombie-skeleton-monster-voices-arcadeparty/humanYell1.wav
8294cfabf6b967c0f669f312d6f71ce4c3d87cd99063d566d612ee93e4fe736e  voices/zombie-skeleton-monster-voices-arcadeparty/humanYell4.wav
f32b96086c4e23e568508bd0542bfdadb34ef0d5dc1fef9094b82dc18aa1c623  voices/zombie-skeleton-monster-voices-arcadeparty/zombieDeath1.wav
e84d3d060439504e07c564a13bd57d55a575dc28c8f2af32f19a1b23514d538d  voices/zombie-skeleton-monster-voices-arcadeparty/zombieDeath2.wav
9fb3eecfeef887007b8f737e663ef52cf6e8419b46011eb04e0775f1d78a1db8  voices/zombie-skeleton-monster-voices-arcadeparty/zombieDeath3.wav
10437ee9e240e6b0d0c910f4586b89f306565c1e97ef7eca2bb0e11a3d6c76a2  voices/zombie-skeleton-monster-voices-arcadeparty/zombieDeath4.wav
8c7a107ab2696b9905bfdaeadad6cc255f091676538b35bc12b0a85ade69e8de  voices/zombie-skeleton-monster-voices-arcadeparty/zombieYell1.wav
d1f32fedcaafce528455d49512e78f5074a10e099536585850470960b5144f60  voices/zombie-skeleton-monster-voices-arcadeparty/zombieYell2.wav
4a205aac389e07f360d4bb5c867e66d10e4539d5aac9283652cbfc0564c80a93  voices/zombie-skeleton-monster-voices-arcadeparty/zombieYell3.wav
79cd27c70ac3b44d4f7e8940310ee21073987032ea299203347639170f8c1208  voices/zombie-skeleton-monster-voices-arcadeparty/zombieYell6.wav
7adcb385a60805b4020e3f973fedc70706f76608d336c8774cd8272646c41662  voices/zombie-skeleton-monster-voices-arcadeparty/zombieYell7.wav
```

Considered and not used: Zodik - Deep Choir (CC0, a 3-minute track) and Crowd Shouting/Speaking Ambience (CC0), both downloaded to listen to and dropped, so they are not in the repository.

## Shared project folder only (`assets-src/` outside the repo)

```
902f447a64171c8099589642d5bf2d1d6e52c40e94d957eb78eae722084b0cfb  ambientcg/Grass001.zip
b325ec2f167dce95a0fabc61a2104641288cf20915419578c3efdb46200c563e  ambientcg/Ground054.zip
c5e4dbd7d5555734498834c1e09b799ef3d1f00c61fe08c0d953b93a8fb1bb95  ambientcg/Rock035.zip
fa7974a0d342bfe63c38664ba9f8ec1a4aab8ea25f099bdc56870e33588c4d9d  kenney/kenney_nature-kit.zip
19b31755ea858960152d2936842522b3a6b823a5435df965498ecc704c7bfd08  quaternius/easy-enemy/Blends/Frog.blend
8b79283a01763cbff54f4774a3b80de8aa121b8484fd04c77ee369439dc7a446  quaternius/easy-enemy/Blends/Rat.blend
0efa2503bfc197cbc26b91d5231b8000c56c9d79be52d34dd0624e38394617c1  quaternius/easy-enemy/Blends/Snake.blend
d16e900d632d821e008f60f7afe239966861d666648c28d1751187e9bcf4a149  quaternius/easy-enemy/Blends/Snake_Angry.blend
728a9c3bbfe8526099be27b6aa3594ed88f1c3648e923e53437f5b42078ad4a6  quaternius/easy-enemy/Blends/Spider.blend
5de32f469e5ffbbf59b485cfd1be6949f438bc94de0685b486435ebee88883dd  quaternius/easy-enemy/Blends/Wasp.blend
e7366c2ed4681aa353a832e2bfee20865717b782f3d256dfeac0060dabbdebee  quaternius/easy-enemy/FBX/Frog.fbx
5c27f431dd08030237d17dfafa8ae0e4657eac97c5d6d07382484c618be17a88  quaternius/easy-enemy/FBX/Rat.fbx
553092a2e1c68ab4e49ce5c139942228a77a911b25c78635aca7901db0a8d53d  quaternius/easy-enemy/FBX/Snake.fbx
2a5d91cd08a1f6940b4ebb9123e6c307e4ee48d4131f82c9d128f9027aad3a3b  quaternius/easy-enemy/FBX/Snake_angry.fbx
01e2289dcc20cb1636b8339be79b9981990bc7e3bb6bda066aa7824caab33946  quaternius/easy-enemy/FBX/Spider.fbx
01a8f18f97732fbe073ba67fc2a79e51f1a71c87fd3210638af63d1d681dd54d  quaternius/easy-enemy/FBX/Wasp.fbx
83d8959f9fc56353ed571fbe2dc52e4bcd64508e2399501cd45ac2ce3df0bf8c  quaternius/easy-enemy/License.txt
a9d4a3d49994b4921baac7178a118ab018c590b0c0c04a263af83970dfe1d1db  quaternius/easy-enemy/OBJ/Frog.mtl
3443d017629bb0b50f00c2a1d24d438720c0beb444bcc0449f8f87a4eadc664a  quaternius/easy-enemy/OBJ/Frog.obj
03aa62c3befe2de1484716eabafcd068d71794b919dda5e8c4834094aea549b7  quaternius/easy-enemy/OBJ/Rat.mtl
f4a1fb3830b1c2e0dedfe2e3e17a20805ca4184b3bbaf1ca68c57450ce6f9988  quaternius/easy-enemy/OBJ/Rat.obj
ace53a3559b906080bbf79f1baaebd88772a495a8b4111cd3dee3274ca86b85c  quaternius/easy-enemy/OBJ/Snake.mtl
0ead0c5a616f0dd0fcf2d55f53543a169653f7c22611a640b590b5f2db8a65fd  quaternius/easy-enemy/OBJ/Snake.obj
edde96ef2b7d446a18a86545f0510399a58cd2f1767eb57a8fbbc32ec5521039  quaternius/easy-enemy/OBJ/Snake_angry.mtl
8a4355054c0fe959c071f013d0938e71be319af2ad01000e15831b2e73e59367  quaternius/easy-enemy/OBJ/Snake_angry.obj
8db84ecdcc89f6ca950de59ce63d7d721ab9b2e0e9bb1411cdab7b240587122b  quaternius/easy-enemy/OBJ/Spider.mtl
86ef3ec7d15529258e51239a5d99e1cdff8a168e5352ccab724c5bbed0e55837  quaternius/easy-enemy/OBJ/Spider.obj
bf08d28176e70621e2546279bf170203554bf0426827385ee0abba1c0c2a6152  quaternius/easy-enemy/OBJ/Wasp.mtl
033b428117e0fd8d1f3c5a4b39cca0c937eddc3d88a8caef43f3526e9a20edcf  quaternius/easy-enemy/OBJ/Wasp.obj
7e1e18ded5419399e8c7216570834d051cbb3893badf2547af6a42ed08f73d2a  quaternius/easy-enemy/Preview.png
a63452360b79e9dbe8e351c94a0fbdc34544b902d4ba94486ffb784a7c3adbd2  quaternius/rpg-characters/Blends/Cleric.blend
670ebd0b41ea60a26bcb7e8e469bccf22321ce3c133db21e0dbf086fe286b0da  quaternius/rpg-characters/Blends/Monk.blend
cad9561272d38339ad3d42d0e1e343b0a2a5ec12dc5b36d498e4e78ae7f7d1fa  quaternius/rpg-characters/Blends/Only Weapons/Cleric_Staff.blend
68d3968c272321d8324f9320bf4743ed7d19a8a38c320a1f6480fe35625ff89e  quaternius/rpg-characters/Blends/Only Weapons/Ranger_Bow.blend
33c88a732c76c5744aa3f82e6b0a08ff21ba618671c9b91c16c4ccc1112c5d88  quaternius/rpg-characters/Blends/Only Weapons/Rogue_Dagger.blend
6c606f81c1c4ab1c75f7b40f683292b23cbbaa6981429dc3955381abf99ead1c  quaternius/rpg-characters/Blends/Only Weapons/Warrior_Sword.blend
44490a45c45e4dc4903d54f0c24b01ced16963aa4191713bfad14f6b99e7e24b  quaternius/rpg-characters/Blends/Only Weapons/Wizard_Staff.blend
36b72e23075bf26d6702d3bc6fd36f639d9507e78007cafa54a25087f05f45fc  quaternius/rpg-characters/Blends/Ranger.blend
a74a69a5f486a2a5b21d6cda81058ee4045136f7c1c858d6d1e6e1f703f04fce  quaternius/rpg-characters/Blends/Rogue.blend
2310b2a910769e3dad6aaf534c5418cca25a1bde3d408231a3c8d632d2d35a56  quaternius/rpg-characters/Blends/Warrior.blend
d3bed214a16d17c7a19ab37669fc1f0e35749d310dfd152795a6bc3d2d0e5342  quaternius/rpg-characters/Blends/Wizard.blend
59c610231e81066dd6e13a8de456914e56d5e3f8f0db63db192a23610113e335  quaternius/rpg-characters/FBX/Cleric.fbx
3780274c2216ecfc939ea87d18606e7c6760167ae5c5e3a075c57231d5d9ad37  quaternius/rpg-characters/FBX/Monk.fbx
452b5dbfce68556434a2e7d7403d9b7c1185130a97a7821841b4c6c5e252689d  quaternius/rpg-characters/FBX/Only Weapons/Cleric_Staff.fbx
c44d7ea8683160ee75c9c62a2a697740f32717ce61e2722ec4d078de37775116  quaternius/rpg-characters/FBX/Only Weapons/Ranger_Arrow.fbx
97db5c58c239f861dfd7a847f7f1a0a0bff8a4ad64e5b6e3c5ae9b309d4a4346  quaternius/rpg-characters/FBX/Only Weapons/Ranger_Bow.fbx
869423a42dd1c66dfe40dab2020c6552c3429276ee8821cf8e063fef0a0bd978  quaternius/rpg-characters/FBX/Only Weapons/Rogue_Dagger.fbx
e9469f80d46ec6d637d53b65d96ce869cd9dc9317a43d9db4956f8eb18d6f53d  quaternius/rpg-characters/FBX/Only Weapons/Warrior_Sword.fbx
105e95225234104d29dab5d65112448d8399c03eb29fe99260cb10afbfd92c32  quaternius/rpg-characters/FBX/Only Weapons/Wizard_Staff.fbx
cecc642a3cedcd319fe0cb07f71985e07c788fe6759ef1cda2240a52ec1d14a9  quaternius/rpg-characters/FBX/Ranger.fbx
4977a959306b16d4429c4beaf956ee7e9815a0056f4ad42c4c164b0556bc8399  quaternius/rpg-characters/FBX/Rogue.fbx
2efc58cabe03e711ca78e4e2d3e3ae1260a1dc07c5f1c1cd943baa5a9f7e0f0f  quaternius/rpg-characters/FBX/Warrior.fbx
72deaa5b0eed121b9eeb9aa3e218217e4fbc31ded9d28aa91532fabfe22b2b29  quaternius/rpg-characters/FBX/Wizard.fbx
5aac8ea7ec2ec91624951b9dbf4cf2d2a2cea095816d7cc4f55306def8618ab0  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Cleric.blend
dd5ab35ba2df00a9de9f477414e5a2b32125ee540c1dbf2ad67bcbd4e5690b6b  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Monk.blend
6161e5973fb7de74a888242a8cacdaa9c185f8eaa51e994ee0cd244fdc606aa5  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Ranger.blend
b3141d499a9ec217ebf5442515ebdbe86c19c2195ab3099ffab5dffedc57311f  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Rogue.blend
0e153c2a0d7bca5c9aa15de4a34107815fd98770d708bd5db95fbabe34407e4e  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Warrior.blend
ba4a1c0ddebc641885aa7955f06e10cc42fbb35be4ba4f46321566f33199aa91  quaternius/rpg-characters/Humanoid Rig Versions/Blends/Wizard.blend
e4d9d95941c41fcd28fc08cb4941df2ee57a29f158129c16704aad505265e852  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Cleric.fbx
45ca525e023cb3599a31108807a1832c3181c4ccc6341921bbd063aad16407b7  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Monk.fbx
ff9ebe806aba0dd9c30b9bd447386ed9fb22117b8fb2f62aa02864399729998f  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Ranger.fbx
8fcd4cda80369bb6ea773baac90b2497c794c244e577b7328e5ae5d52415de33  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Rogue.fbx
f3421efa7c56d9476932ef273277044bcdda64074cc6c332af99a036c8f4117f  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Warrior.fbx
e86bf77b8b7e8d1ad26a97018ef92f3e5d0c014901208077d16489c7a8b458f6  quaternius/rpg-characters/Humanoid Rig Versions/FBX/Wizard.fbx
83d8959f9fc56353ed571fbe2dc52e4bcd64508e2399501cd45ac2ce3df0bf8c  quaternius/rpg-characters/License.txt
c5b3ea4d2851b9875a51691f8c2641123fad664ad50e9d6b4c520374cdac5fd4  quaternius/rpg-characters/OBJ/Cleric.mtl
d38a911dc2678f99831b1b42d364c88b0e9ad8de48b0b4bd1478c081748574e1  quaternius/rpg-characters/OBJ/Cleric.obj
a492abcff50535229712cd45d134a5ad418bc6b1770463b2f0e958afea6ad18f  quaternius/rpg-characters/OBJ/Monk.mtl
b4ffcaf879323380033a36a71287a4a7f81fae786fef47a80e27b90c48bd7e9a  quaternius/rpg-characters/OBJ/Monk.obj
2845c9c422036a00d0a52eac969036fa5edf64e9a013587ee8ca4e5257a40d51  quaternius/rpg-characters/OBJ/Only Weapons/Cleric_Staff.mtl
e39c32b4c5860cc13758e40013b05cce91ec41db9a39af5d0ab627e5bcd7e7df  quaternius/rpg-characters/OBJ/Only Weapons/Cleric_Staff.obj
b2420523fd1b41b5837a33cb72ec1ba5e8e35b6dfbfa88e0abcf06281f02bfa0  quaternius/rpg-characters/OBJ/Only Weapons/Ranger_Arrow.mtl
ff03553d2afb48722c304faa0754575bc977cfe515b5989152390bb2eb81365c  quaternius/rpg-characters/OBJ/Only Weapons/Ranger_Arrow.obj
b2420523fd1b41b5837a33cb72ec1ba5e8e35b6dfbfa88e0abcf06281f02bfa0  quaternius/rpg-characters/OBJ/Only Weapons/Ranger_Bow.mtl
f00e6aeeaf37cc221813a54b8945f6a2c96c38d6d41be92b2595a8cd30701edd  quaternius/rpg-characters/OBJ/Only Weapons/Ranger_Bow.obj
2c76e80f35d68cba022c22cc0e0426a9a77a1fcd9cd969dd509e9873be735d9f  quaternius/rpg-characters/OBJ/Only Weapons/Rogue_Dagger.mtl
c0c2d85c365825ed4e6d720358069d262ee34984b63c98e6d2f8e35f0d5346f7  quaternius/rpg-characters/OBJ/Only Weapons/Rogue_Dagger.obj
df348bceac4f206427d405a35e52a239f9bd564dab3c97cdd063e92902a657f8  quaternius/rpg-characters/OBJ/Only Weapons/Warrior_Sword.mtl
a92152274eaeebbc03900a470236472b2ba0850b11d26287ddccd6dd1c69face  quaternius/rpg-characters/OBJ/Only Weapons/Warrior_Sword.obj
71439192a8a7b50231d2755d354420d134aa157a4974aa915eba826773bcf580  quaternius/rpg-characters/OBJ/Only Weapons/Wizard_Staff.mtl
d0b0ec2e8fe79cc030a74a4b55490a80b59e4aa89f91689167d1dc98001af3a9  quaternius/rpg-characters/OBJ/Only Weapons/Wizard_Staff.obj
6714781751e2ef048bbdc0255ac47c6f84737d029734a435aaa70c9b5306761c  quaternius/rpg-characters/OBJ/Ranger.mtl
41e2c8171ffd1014c724f078c7599050ff51b3ea397decd2eac9c4d277fe15a2  quaternius/rpg-characters/OBJ/Ranger.obj
e88fe4589da4639cd7ad8cbc4fee63e1bc503713f7d80dde701ae5ad0cbbc09c  quaternius/rpg-characters/OBJ/Rogue.mtl
00ece811a50e0e32a14fbd13d034336b8cbf8f2eb230f3c596bfa90237af1a74  quaternius/rpg-characters/OBJ/Rogue.obj
d94829db1d703ab6c1384e12858036a0b78648eeba34ad32810cc4e401ce491c  quaternius/rpg-characters/OBJ/Warrior.mtl
22765b69f0ebd38497e0152ec35ffa75208a444ed4ffd5fec6965bb9ff49214d  quaternius/rpg-characters/OBJ/Warrior.obj
46b120051833a652376d4cdaefc03807cb8316b3f64da56b760f06f925583555  quaternius/rpg-characters/OBJ/Wizard.mtl
eedd0e442b17567f78c0d96ef0c032ee2247c7984af2b6e1672438aa33910943  quaternius/rpg-characters/OBJ/Wizard.obj
c1188e95990a8b89ef62bae1caa028ac0d2b93967edf9c85d90374d395667212  quaternius/rpg-characters/Preview.jpg
68a360cba89c1b1b775d15ed6e5d4156d05a84a4a1eacae1e5a3dc4f71a61c82  quaternius/rpg-characters/Textures/Cleric_Staff_Texture.png
3ae2f0f1fe67285b16786888b2510fc055d643964dc0be94728db4e8e1e1896a  quaternius/rpg-characters/Textures/Cleric_Texture.png
2853f937515b4f509de1701090d062dc290c003e22cd53c593825a2f6310dcbf  quaternius/rpg-characters/Textures/Monk_Texture.png
c7165eb2bcd4a056d85512c5cdca6bfcdaff5642c7ed77693640bd6a86611de6  quaternius/rpg-characters/Textures/Ranger_Bow_Texture.png
19c4a93a9b96aa0a63248a9a7cd228ed41e504b4f4835599f96936c73bb085fa  quaternius/rpg-characters/Textures/Ranger_Texture.png
f71a1e6ea486f387bec1457aeb91cec9b52dfe0f47d1a28ec9658fa789184bd3  quaternius/rpg-characters/Textures/Rogue_Dagger_Texture.png
fa5f587b8afa0028d168517287a21f2ae4f0f39583c8013944a999abdbb21d32  quaternius/rpg-characters/Textures/Rogue_Texture.png
e46037208c28d1dea057281d0b03f0386936adbf16c6049b79c6f6d243a72440  quaternius/rpg-characters/Textures/Warrior_Sword_Texture.png
dd89b2025be39484a06ea61124ca09a3a2d4df70a44d8eec7ea964fb02e60e9d  quaternius/rpg-characters/Textures/Warrior_Texture.png
7938919bac0a74443f0b92aac39af28ae92203c67f655964f2dd7a0e9d073c5d  quaternius/rpg-characters/Textures/Wizard_Staff_Texture.png
4b8144896832263876a571d3a3c10ee17659f20065b52a6bab7a598ea79320e2  quaternius/rpg-characters/Textures/Wizard_Texture.png
6497456a3fa1c151d7a6ab4f7b4c343ee416c23d9454505b950c93ea558e4601  quaternius/rpg-characters/glTF/Cleric.gltf
085c049ebb593e0edbef9f0dd1f62fce70f668f6be7be99f3a1aa5378c83e8a2  quaternius/rpg-characters/glTF/Monk.gltf
2732741d5e21c20760d7f3dd9d71812d4b796f466be1a6cba212f45483dd56ed  quaternius/rpg-characters/glTF/Ranger.gltf
5a9eaca0871cd69f554747a1d42870b3bca702ba721d941d909fc530cf18afb8  quaternius/rpg-characters/glTF/Rogue.gltf
2bab905ce12d279e1d099c2a8de0b1ef31c9f5e8d1959b41d24470b4f4181add  quaternius/rpg-characters/glTF/Warrior.gltf
320de17289857db6794e4eef55101ce3427e755319a4384fb6d9d9ca5206b41b  quaternius/rpg-characters/glTF/Wizard.gltf
25252b3fb5d48b59f2b2dafeb0fd61af0c3ea652254b39cec02fc752be0123f4  quaternius/ultimate-modular-men/All together/Blends/Humans_Master.blend
39b729db833fafb9fed5a5da93818b458675d519508195f128099f29c4d93775  quaternius/ultimate-modular-men/All together/FBX/Humans_Master.fbx
9b684add13cc5aa20e7671bd1f47202d762bae1839c00a247fcdd57e29a37439  quaternius/ultimate-modular-men/How To Use.txt
1c4bd98437d0611be72498e8caf402adbd6f11002b244b35e8d9cc22e6a541bd  quaternius/ultimate-modular-men/Humanoid Rig/All Together/Humans_Master.blend
444a74f3bc20bbe02714e2894f6f6f95f44706f5e85ea9e85d00c235f53a4b23  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Adventurer.blend
2e8e3c8a9067858cf198871ba0b132e5d174796114f3c2ffaf0ab8fb825dca24  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Beach.blend
0320cfab3c1089bae100daac0d9307319c89e262d4ced43693a849158a403253  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Casual.blend
debdc9a89261e3fba889effdfc30b6f06b481076d8e072679e660a99a8b5c7a7  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Casual2.blend
54d220d17175a954134b8565cd75f930e05cb155b17d5f41673fff79a9d79087  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Farmer.blend
0a59c2dab5689dbe34d22033c77e148e687b07d7cf729061c342d4b5051f72df  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/King.blend
ade5867830ad133f16e313fd662396ccf100d35b5e2abe80009eaa7ff3634f5b  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Punk.blend
6c776d16798ba8702506aa5129a70552d1e2fa8c759f1d5354dcb375cfe1d167  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Spacesuit.blend
f8a406f2d916c3a4c1ed7fdd9f757c8fb2f33f2bd4cec967eaf8c479fc1a977d  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Suit.blend
9e866fb568cd31e0e934b7bfce9167aabb469dd911bf524c3813d9c0889241c8  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Swat.blend
6ae9a1a412ef1be5c7f2c3ef4d57910212fee19c0fc933e55d409555487c8a72  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/Blend/Worker.blend
2dd4bf1bc18968fdc5245863c25dfedcb572dbd63ae7581fe27ae4a8a063d6e3  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Adventurer.fbx
2ab227a41c6a8a594c61828fd63beec69b0d5f3d23edc2fd5a938e5859322862  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Beach.fbx
a4f84c2b788d2761a711cebd628c1770b7ae5a5a40e7a8faf09dd5c37a8eca7b  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Casual.fbx
16087f97266eab15cbc154859dc9cb8c0dd068f7adcaa6ad4be685744bd61d49  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Casual2.fbx
76d57bf99aa30146b9376ecfc903fefdef67f8a293d82de323e2dd660933a352  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Farmer.fbx
2f428a56c253e01d48501c8d9a63b967b74357beeac9f830df65f7ba68f51c5b  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/King.fbx
59b0ee5aa13c02398e818abb6bb9011e2b95bd6b5ae0a8f8014c4a3553434993  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Punk.fbx
9b8f320e3ee853a928a4d3abf646690eeaac5b2918e0d5ef3a31a526c2953548  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Spacesuit.fbx
fea0cd877573ec24e8882f3165708a1c6e3ed1aff7b886b3d867820dee916413  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Suit.fbx
3c5eda6fe78350350363cb1e2ff8be0c7d8a02d58fc60584992b471c04357fa0  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Swat.fbx
000eb1269f39e0b10a77c9bd33181c790341480324d72708cc7c22be177c49d6  quaternius/ultimate-modular-men/Humanoid Rig/Individual Characters/FBX/Worker.fbx
b62466a59b8b070e50593b696865eeb822dcd96ea7925cce68ade9cfc3261d8b  quaternius/ultimate-modular-men/Individual Characters/Blends/Adventurer.blend
94e0da4781462a9e2702429de230191240209ab670f5ec0528227b0fa1019e56  quaternius/ultimate-modular-men/Individual Characters/Blends/Beach.blend
8b033ab6343c8ef095fb6ed1bb46ea8308f449ae2b7241b3a9b1413314e3d016  quaternius/ultimate-modular-men/Individual Characters/Blends/Casual_2.blend
d138d37cfb3f71400455bcbd9363a69d6e3e6f7288aede8b5bfabcf1d3a14816  quaternius/ultimate-modular-men/Individual Characters/Blends/Casual_Hoodie.blend
d3cde53e62a9fdd0b4f87b7e9d07375ec49032ac02fbf36fd15aed6d25048876  quaternius/ultimate-modular-men/Individual Characters/Blends/Farmer.blend
d9c4569ee4d6c1051ad2bf1dce37a98a8e9104b1d830452f1b77e4da23631c25  quaternius/ultimate-modular-men/Individual Characters/Blends/King.blend
c6e38e88eca44194376392b0050655f2f17045a198e51807b2acfc870adba1dd  quaternius/ultimate-modular-men/Individual Characters/Blends/Punk.blend
8725db0eb03d5174514c4f456d0eb2f809d3763a2aed06c1a3ee0b7f7bf987fa  quaternius/ultimate-modular-men/Individual Characters/Blends/Spacesuit.blend
e1e74a71661f4836bd933c2c9c5129c5f98474f92a3b7acb73caaf439f0e722f  quaternius/ultimate-modular-men/Individual Characters/Blends/Suit.blend
189bd266cf42e793b7d713d9fd07abf18a9f5c6c829432f9c652cb9615d40792  quaternius/ultimate-modular-men/Individual Characters/Blends/Swat.blend
accf9486379fc00c9159878649221beca20e6eacf2ed0dcc5c248a2bad6fa447  quaternius/ultimate-modular-men/Individual Characters/Blends/Worker.blend
b3a28d7a6578e2039908b11d1516c98130f7b4f1a5553141be51349c8fc63bf3  quaternius/ultimate-modular-men/Individual Characters/FBX/Adventurer.fbx
74a9aa79850d9871ba417718792bb7406844af200fea1ba48c0f977e02030d44  quaternius/ultimate-modular-men/Individual Characters/FBX/Beach.fbx
9b4c6520de636575d4ba612e3db0716306858b24871da8c0ce039d914523e694  quaternius/ultimate-modular-men/Individual Characters/FBX/Casual_2.fbx
55fd019ef45766267e4decf101899b8c4f6019d077cef9047a6de1b7a2120b41  quaternius/ultimate-modular-men/Individual Characters/FBX/Casual_Hoodie.fbx
2dc21f6ece90959015b55c0e42917a02c9d08e2b890b840df386b3b7971378d1  quaternius/ultimate-modular-men/Individual Characters/FBX/Farmer.fbx
1548dabf222d4d7785706bbcb077e744df7724c042f59449032e406cf0de9838  quaternius/ultimate-modular-men/Individual Characters/FBX/King.fbx
b55a6f4391b4b0a8314cd50947c84bcfded7510a371a8113a47870b74234d121  quaternius/ultimate-modular-men/Individual Characters/FBX/Punk.fbx
c90accfe763e245f0a1ed8107b99fb710541c5f432b96e178b21e72c2a7cba34  quaternius/ultimate-modular-men/Individual Characters/FBX/Spacesuit.fbx
75adb87a522c0af711890db5afac22259f4e7fda5ec5173e9af09d1581704268  quaternius/ultimate-modular-men/Individual Characters/FBX/Suit.fbx
c4f76953cf9b5b6625e75c92b5cea3e3f5c1420df26c53934b7e55cc05e0813f  quaternius/ultimate-modular-men/Individual Characters/FBX/Swat.fbx
41f5e13b9bea4a990fd0552a8083e2ce6375ed30b1b1775f3644891e6b4a1bbb  quaternius/ultimate-modular-men/Individual Characters/FBX/Worker.fbx
21f7a61afb6bd6cef6961490c367594e3c2fc01ec1f041662131172ce763063e  quaternius/ultimate-modular-men/Individual Characters/glTF/Adventurer.gltf
76e001ea131fd76a1bd938a7862606cb8037f7049b632783580b9bf4da2371a8  quaternius/ultimate-modular-men/Individual Characters/glTF/Beach.gltf
55c654d09a2a5ff6e3bd6158d4a1b462f181cd6f1e12a0f5e9d959f9c3abc438  quaternius/ultimate-modular-men/Individual Characters/glTF/Casual_2.gltf
dd74886c26998a0fa888b4ce557a0932d7d97b0265dd4c763154d081b7a6cb98  quaternius/ultimate-modular-men/Individual Characters/glTF/Casual_Hoodie.gltf
46d3e85fa8d848ee479ab8e4672c16724ee4ea51f3af5733705a153937f93555  quaternius/ultimate-modular-men/Individual Characters/glTF/Farmer.gltf
659c7d84dcea8c6331698c3430484861944c83e629389425664b782d4aecd17c  quaternius/ultimate-modular-men/Individual Characters/glTF/King.gltf
f9224072f5e6cbb207eca250faa7f1868614a1a984074fe79c7b2862df4feb42  quaternius/ultimate-modular-men/Individual Characters/glTF/Punk.gltf
33e0e0fbc6140ffc936a099fd84a274be406115c4b32aa0a697641710b67392a  quaternius/ultimate-modular-men/Individual Characters/glTF/Spacesuit.gltf
6c89fbb31b96c1a63ad94e3dee0942bd7b34bc789a5d39fd6a6a1738a9214fb3  quaternius/ultimate-modular-men/Individual Characters/glTF/Suit.gltf
622b3f36fcac90539ef8f7121ec1f11b5e1ae603a085019b3fa96eba20dddfd3  quaternius/ultimate-modular-men/Individual Characters/glTF/Swat.gltf
e49f8ec0f8a7de72dd26b1c01e6413c9a87a9116eeee21f9364ccd36bc286335  quaternius/ultimate-modular-men/Individual Characters/glTF/Worker.gltf
e8dbf915a2b82229913e301a0787696611241bdefec4832bc084f54161db1efe  quaternius/ultimate-modular-men/License.txt
2b9dcde20fdd6f11fd050e82690ed7ac8c6a8ed61f9efc72b27715b893bdbfad  quaternius/ultimate-modular-men/Preview.jpg
b94044161018597daae96a4594789bf6d5149995f07bd874a358ac8eb183facc  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Adventurer/Adventurer_Backpack.fbx
08bbed7cf27467c5d3ce267325176b9db0a3484a653df7a64ea9a219f8a17c42  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Adventurer/Adventurer_Body.fbx
2f70f71761d26f2d9b673cebfe0092b2468e35132a038cd6a37d518cddc86ae0  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Adventurer/Adventurer_Feet.fbx
759f7f23de24d3d63005d30ad9578acedfabfa7e8a03802fa4e8d17f77ea3a43  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Adventurer/Adventurer_Head.fbx
d9d81eca70a7e87e40302f0785f8ae2e82b0057d94726a676ba81d12cf358651  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Adventurer/Adventurer_Legs.fbx
259ed6f6e5f1bb2a15215a66a9ed02e8956083a281a8098103490d1a00292b7f  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Animations.fbx
917fa30154bdb1171dd9f55d435f59add2b8923cccfdc425e0aaa27fbc552f3e  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Beach/Beach_Body.fbx
c0982de32a0a0e51dc171884b2a2ce615b313f6224706b8c8c0ade29223da19a  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Beach/Beach_Feet.fbx
aea5839b3334fe84796d1c4a91f097a6938ebcc05c168e4f338247331acc2450  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Beach/Beach_Head.fbx
245810d64690090df011b89f0694409547e794935599534c64379ea51fb234ba  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Beach/Beach_Legs.fbx
164160a8a8d4782e458fb26a78da24ecd9d6efeb9442263034bdb53f04bb9032  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_2/Casual2_Body.fbx
eba4aa8849f1a705f60f5faa9547bfe9a32f58a35526b1cefbe785bf1537ebb4  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_2/Casual2_Feet.fbx
409981af4a87542785cf2339f6770d31307f8acee9c724ed78a43a029b849497  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_2/Casual2_Head.fbx
b6f2a124bbd98f9c903aca5dd82caea3fd7c22a3e7debf23823b85d1cbebaba7  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_2/Casual2_Legs.fbx
446c8496c80d2b179051c3dd3e94210d6a0dadc697300c0ce0f3b8742b272dc4  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_Hoodie/Casual_Body.fbx
c7f2eca1e7c6f002b568c28d2bad140164b21e5bbab8d603fd4d55e7786f1c97  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_Hoodie/Casual_Feet.fbx
d81b37f8b1bae73b0af09466b9c0acfcfe33745312b1746061c54f8863bccd3a  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_Hoodie/Casual_Head.fbx
ab923176e06b9b3f25caecb2f1bb441b9d3bed73d27c3fb19745f02dd1cc6da1  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Casual_Hoodie/Casual_Legs.fbx
7e5af5ce3328970aed1fa4702e58f0e7a48229d81155e26b457ad007e7f468d0  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Farmer/Farmer_Body.fbx
3c458b1b2a66830c44c7b76c123c68d63c9f9e7461362efe0c33ce4d18a276e4  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Farmer/Farmer_Feet.fbx
3a80c5e69c425e880eab975fdaa3f8bb2a2e0746c2652a67f1870917f3f6fe41  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Farmer/Farmer_Head.fbx
2adadf445fc8f6ad275005f103efdc31ed1cb5429c787a35c599c313ce44c105  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Farmer/Farmer_Legs.fbx
5a6f479538ac5a035dcd3e2a11f3f1139db9582d5269bd0ddb5b13268f4cd04f  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Horse/Horse_Head.fbx
94542739fa8d7f441afe1241a6a4d65f57051160b7442752598cb6d0bfb91882  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Humans_master.blend
a9e00810a21acf6d5a6ca0dd637ca9e7346109a12e28c3fcde4eaf5edcaaa83e  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/King/King_Body.fbx
63aa3b50da1bc3901dd6b8ca62597290475cca3a2392c035647b145b1042a68b  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/King/King_Feet.fbx
3fdfa851b779da708ca685d28f438d1bfd7c678ba221688d83e7463d5c32b11a  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/King/King_Head.fbx
7a2813bf4bf67593b2448312e41169a32f9a2ddca07717e1041156533f966d9b  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/King/King_Legs.fbx
8a918b6104d0a82681df830ea60f8194a952697718163f4c086d8902d5de2a94  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Punk/Punk_Body.fbx
84d6d2199cff1612ec6abd1fc0dc09dd6995d4ee711a783898123515d510c16b  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Punk/Punk_Feet.fbx
5c6fce42215dbff15554184549a455afb709b5f954b1758b9d03a2a64938c01f  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Punk/Punk_Head.fbx
3d05512969fdccd7b83719f9b4c9d3f1ffb33299e5fa3bc4e16b667f656767e8  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Punk/Punk_Legs.fbx
e14f62b4c2198631a5b387912c33324eccd9541a126a5e613175c3eb0be21aed  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/SpaceSuit/SpaceSuit_Body.fbx
6a6883b60a1a1e295636c963fe4e2c1995d07ea6aeada33242e1c9202e4dc4c8  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/SpaceSuit/SpaceSuit_Feet.fbx
da77cb4ddf6e6d83087c982d775755581c6385f1ec78c8043d503b8f67cf9941  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/SpaceSuit/SpaceSuit_Head.fbx
060a248ffe54e279b1497572e2c2d73bb2b2fdc47a8e1ec2262faa800cf46d36  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/SpaceSuit/SpaceSuit_Legs.fbx
73846a4cb0ec2fcd2829c774c69b3914c21a6ebafabf94ea8e896161fe045745  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Suit/Suit_Body.fbx
2478e4051dd3b7f832d775d29db6cadcc23188a2401b2acacbb0e23db2af015a  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Suit/Suit_Feet.fbx
8efc1f267ad198b466ab3fff04fb9f656aa6879e04b2150fe2810a27d4123430  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Suit/Suit_Head.fbx
74301bb3cbf359c5d276b4d61931aafdddb9d80f6b383608ccc1bf0c124dd5e1  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Suit/Suit_Legs.fbx
629d35234d4579b949d10086ddf8d938bb07a06842d8b3217b297262e6a01090  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Swat/Swat_Body.fbx
1f6613779a2637809c7a148c12fae5fdacd262fb1707fd50ab72c2d84aa740bb  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Swat/Swat_Feet.fbx
efd79c4d709814176255eb514f9623cb172c30ffd233fee946d01a0cfd6945ac  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Swat/Swat_Head.fbx
15f0f8f5270a3dca650e52f1b635c2b1e20730162314e66d463f9180935cc2db  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Swat/Swat_Legs.fbx
cecb3c460ceff92ae56a0c678a66cd3d9ac8067daa524b9107c7694f3a8353b1  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Weapons/Pistol.fbx
e9148f0a58a780bf4961502917f148dc4c5884bcfb8e0ed3c4caf8b0640f98a0  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Weapons/Sword.fbx
069ab65c35f3d2ea7a46b84fbec97025d3f5312893c8061779a516836112ab48  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Worker/Worker_Body.fbx
a4fc307ed84819bbbedaa5a8e8226505dbe585d8f97aee2e29c3f0b1c3e9e28f  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Worker/Worker_Feet.fbx
12f57b247785740196717060cf3d9c26cd901a79b3bc77c40bc3bf7f91cd883e  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Worker/Worker_Head.fbx
812c4123fb4087eed0cd5d8f7df16b6a6fc678ec3b6f4edfec0e35e5f51ab464  quaternius/ultimate-modular-men/Separate Skeletal Meshes and Animations/Worker/Worker_Legs.fbx
05a0d1156614968abc756428bb54015d90f825f9cc14739a32cd5764691169c2  quaternius/ultimate-monsters/Big/Blends/Alien.blend
c6d9a681ed0987c771928a76dd96f7dabd11176114fa39d2fcd5a2dfface80f1  quaternius/ultimate-monsters/Big/Blends/Atlas_Monsters.png
e1f2c2fe2681de7783a7c43bce53ce0a6bd189752ef3c037f2415dfe1e7dbc84  quaternius/ultimate-monsters/Big/Blends/Birb.blend
20a5b0c424ca2b03f4d4ee457d762e0f2cf236f410d4ac54a6ec1b6c66ce3346  quaternius/ultimate-monsters/Big/Blends/BlueDemon.blend
f39fa3ea2d57955c5313722664796c8fd55e92e478d8b982da18952721472193  quaternius/ultimate-monsters/Big/Blends/Bunny.blend
819a5c58883323ae735d980591e493d6bc284605977f12c71817d9a3a5abc1d4  quaternius/ultimate-monsters/Big/Blends/Cactoro.blend
86a9aebbfe9fd4ae55828517f101867338848f4096e2609117c3c0d68520e09b  quaternius/ultimate-monsters/Big/Blends/Demon.blend
47e6ad90a8bdab41a13186911dadc40e8bdcafa0958aeeabcbfb9726f58027eb  quaternius/ultimate-monsters/Big/Blends/Dino.blend
c63643b64df6bba7a2eeb2c6f69e3533f5c7360ef0c7b1ee5775d05ee23bce4b  quaternius/ultimate-monsters/Big/Blends/Fish.blend
4af32f5d9b3084c242afda66a2269ae13ea78501b023d5873dfbf210bcc89f3e  quaternius/ultimate-monsters/Big/Blends/Frog.blend
ce511c1eb19db09b11c9726e033b70f1002ba72de60591883b412c7fe5edf3ea  quaternius/ultimate-monsters/Big/Blends/Monkroose.blend
6b100c6e8e70db9a2a2a2f07cf5a6a00ae327cd9258e22fbdc6e57b1876526e6  quaternius/ultimate-monsters/Big/Blends/MushroomKing.blend
02b47134b1c33a1cbeb6c5e7dc5d0c54a17ff9a6ac740f9dbccef68e15d9f521  quaternius/ultimate-monsters/Big/Blends/Ninja.blend
88a9ec1b17044b254dd715f7068549d1a1a9413d033e0ae0136f5267c00f12de  quaternius/ultimate-monsters/Big/Blends/Orc.blend
6f3bfaab1fab8f0f962818d5adbe7035d056a1dad7f847dff769b16f4a7fe136  quaternius/ultimate-monsters/Big/Blends/Orc_Skull.blend
0409aa0cdbc52fb22819dab67f10d1a7c3dbecf07cf1a7cdc5b2aeccc1efe6f1  quaternius/ultimate-monsters/Big/Blends/Tribal.blend
c093bb3e81cfdb323465c9c550f1a5cf15d0af8b2bb8072e59cca1b585625f76  quaternius/ultimate-monsters/Big/Blends/Yeti.blend
060c6e503092fc48e0d552ecfeead953442b7e154181bc12e5da03ac6cf716d5  quaternius/ultimate-monsters/Big/FBX/Alien.fbx
6c216066eb0af92b82b9c132fbf9437f12fc2fea5d8e0184a72f2f7b5baf4484  quaternius/ultimate-monsters/Big/FBX/Birb.fbx
2e84978f3455192a4c56e979acd4339ee1d209e0d866d1dee732666089c15f7b  quaternius/ultimate-monsters/Big/FBX/BlueDemon.fbx
dba315260399af68b08ecf559534dca3a21fd921770a8888afa84fc2d86037c0  quaternius/ultimate-monsters/Big/FBX/Bunny.fbx
109561ce7e6db75f5f5238215fed0fbfceb171151db6dae5d02997523b8a95ae  quaternius/ultimate-monsters/Big/FBX/Cactoro.fbx
88ca081ed981e44e79dc9bb0ccca58759848f70b300e4893c3a7a34a3ee4d5af  quaternius/ultimate-monsters/Big/FBX/Demon.fbx
6824daddece50a5b89ce685c2b80a085bb6d59f10920463199a7b342da500fd8  quaternius/ultimate-monsters/Big/FBX/Dino.fbx
2aec5899afce298af0ac187db80254d84746ccf42a68e1c55086c5e7b5806c4e  quaternius/ultimate-monsters/Big/FBX/Fish.fbx
77c8cb364de1f63605fb5411c70237d02aa416a6ae118f2db3c7e88c7e5be897  quaternius/ultimate-monsters/Big/FBX/Frog.fbx
00c225a500b15bd360cd903bcc0a0f84439395eed7e84bfe413e058f4125af4c  quaternius/ultimate-monsters/Big/FBX/Monkroose.fbx
a7b6b869d696e1ad0d01d5294a4d92e271858b04175408ff76fb05d946669555  quaternius/ultimate-monsters/Big/FBX/MushroomKing.fbx
9038cca159b4356044d08c03f3a7122cb8660ffb84c5d8813cc4d77460ba83da  quaternius/ultimate-monsters/Big/FBX/Ninja.fbx
571811f39af3741a8bdc299f6022f0e3136ecd95891aad5f0945479e149c5981  quaternius/ultimate-monsters/Big/FBX/Orc.fbx
9724b8659bda67fb8664af54467014c19517f3773e5224f1d09ca5da880d44dc  quaternius/ultimate-monsters/Big/FBX/Orc_Skull.fbx
40a563ab109ca0fc59969c3a226352fc1eb0b00e7204795e714992b30a5ed2ac  quaternius/ultimate-monsters/Big/FBX/Tribal.fbx
4a8cac621c9595f4ccb19c16e4bb590014c01b4b2994674122b0caf2b74de58d  quaternius/ultimate-monsters/Big/FBX/Yeti.fbx
6b4f11e55ec51a5f580dd00d9b19cf74100fa81218d00ec2646f771262681ac8  quaternius/ultimate-monsters/Big/OBJ/Alien.mtl
747d6bd7c5e157caca10b986783ae9ac39aa74bbfe1a8878a5f2d14927d0e12c  quaternius/ultimate-monsters/Big/OBJ/Alien.obj
e5a8595ca64771ced87279f3503a172a7b30c776618b3d71a2ee85587bc11edf  quaternius/ultimate-monsters/Big/OBJ/Birb.mtl
72650db32a5dd122e9905feca8e751e995693adf6414fbd20278067d785c0636  quaternius/ultimate-monsters/Big/OBJ/Birb.obj
15d285e470d108980c0f56f135bbd9a24167a62c6a10c048e17a7c1fac2b4eb8  quaternius/ultimate-monsters/Big/OBJ/BlueDemon.mtl
4f0f5bf8a76b0101a46e35fcd9050f7aedfab4126d6212d84bc2052786610402  quaternius/ultimate-monsters/Big/OBJ/BlueDemon.obj
dccd81248589fa71810381d54ca105d0ed8851dee6eefddb3d4973959e0990a5  quaternius/ultimate-monsters/Big/OBJ/Bunny.mtl
0d36fd319a7015e3e4b3d897c6448b81bcb4c6ef486a134c0eb58be9da869dfb  quaternius/ultimate-monsters/Big/OBJ/Bunny.obj
9e3afca93c8e6af70c5437ff7436f235deb43d2cc32afe978ffa5aea767fba8d  quaternius/ultimate-monsters/Big/OBJ/Cactoro.mtl
4d3f1cfeec92bb9e5fe0f252b7f4fd49d1905c3896981d0b2d99df99e02eeca2  quaternius/ultimate-monsters/Big/OBJ/Cactoro.obj
8d1db06eb47c885e3c8e8e74c2eea63f5ef4294b3e0f72531d3f3980f89e8114  quaternius/ultimate-monsters/Big/OBJ/Demon.mtl
043f5985131f717622e89828ed1b7214a12363f4946bf882616655921825a01b  quaternius/ultimate-monsters/Big/OBJ/Demon.obj
95de4e2e7d8cb0c660c248f04a3e41f6c90ac88fb79b1708e1bc62f642494b02  quaternius/ultimate-monsters/Big/OBJ/Dino.mtl
436cc7df542a43e44e4ba35a436bc07bc1f8fedd7619803806b86e133898b5ce  quaternius/ultimate-monsters/Big/OBJ/Dino.obj
b33748944888f426304a34aef97d18a1e5f0cadcde811d4fafc279764287f3d4  quaternius/ultimate-monsters/Big/OBJ/Fish.mtl
8f1b879761f65654d3c2f67dfad6d3c54ac5293b0597aceb83bf255a072ce42b  quaternius/ultimate-monsters/Big/OBJ/Fish.obj
64324f8a0ee29bf44d583572e0ab3102df4d234dba2f1e601febdb0f53661262  quaternius/ultimate-monsters/Big/OBJ/Frog.mtl
c58cac19040b5cd44f68e5e3743a3a64cd6f44fc9b4114447b2110c1123eca4e  quaternius/ultimate-monsters/Big/OBJ/Frog.obj
39005a5d015ef75ed2cd6141879305e60c520fd79174fe6ad77418290ab0595e  quaternius/ultimate-monsters/Big/OBJ/Monkroose.mtl
6ac14e24323a56d2a99ec9b6250ee05a8715d9047b4a478e908f0df4e3a78ac1  quaternius/ultimate-monsters/Big/OBJ/Monkroose.obj
d67447bc241f443409811bf41500892d0c3c6b11d315277d164bcb2040de332a  quaternius/ultimate-monsters/Big/OBJ/MushroomKing.mtl
bc509a75a0a5f0e64e71747e024a4f0df562e2a7d9e372ad0ad56e682b7c1a37  quaternius/ultimate-monsters/Big/OBJ/MushroomKing.obj
5b06f442345e638ecd25e81cedd79a3f0281264b8994a6b0c5f9efce18c1e601  quaternius/ultimate-monsters/Big/OBJ/Ninja.mtl
99b084cd29ad3f40984396903f64f25eeaaf0618462bd7fdec2f1a313f14b8bf  quaternius/ultimate-monsters/Big/OBJ/Ninja.obj
e6fec42f9d4db3c3177da9027c5cdb2c4abd71cb934ea4f4788aea155f267124  quaternius/ultimate-monsters/Big/glTF/Alien.gltf
b24b3117ff13f91adfa9abbac82cb0031f1cbd514c6cd187539005be2ea49d91  quaternius/ultimate-monsters/Big/glTF/Atlas_Monsters.png
d2422f23943125b397078377026172d36ccfd5a657057ffc3803be4a9e983307  quaternius/ultimate-monsters/Big/glTF/Birb.gltf
9eb899fc619dff9a973669e0962452990c8ec44d4501dd41fe4f4fe5afd631f4  quaternius/ultimate-monsters/Big/glTF/BlueDemon.gltf
89fce228bdcefac4e498378fa061b9d928bca01b680101530648f8b28c9c9ad9  quaternius/ultimate-monsters/Big/glTF/Bunny.gltf
29051fc39751fa5b3af69f7bce77297c620651b1b2235bbd644962bd6a895461  quaternius/ultimate-monsters/Big/glTF/Cactoro.gltf
9ce361b41a0e80d42a70f6325169b9c70a6e3304b8d856b8344a2b7f185067af  quaternius/ultimate-monsters/Big/glTF/Demon.gltf
d7c57a4ad6444f9c845ecdc13782a7430fcb377594a00f719cd2d37b9434b6de  quaternius/ultimate-monsters/Big/glTF/Dino.gltf
9549f80351ad7c8e97c6894662a80fb42e4df8ebc7588b6912ea8f9e2ec6b9b1  quaternius/ultimate-monsters/Big/glTF/Fish.gltf
8a73522affc7483fbd43736e95d5581ff993685312995d2c7ebb112eb7d5c004  quaternius/ultimate-monsters/Big/glTF/Frog.gltf
4e984e22bef1a49da0ff92154f5747d3e61997637937532bc066867f08e8e719  quaternius/ultimate-monsters/Big/glTF/Monkroose.gltf
ccf27e126b881d80a2046c3ab96ddd94eb996837480e783142009ecd6629a3a4  quaternius/ultimate-monsters/Big/glTF/MushroomKing.gltf
f1042e533bd24b98ceee179eaf4119517051fd745feb8d71af1b78a4ac3c88a3  quaternius/ultimate-monsters/Big/glTF/Ninja.gltf
e61a37f8d9b2eee28928bdfad2c55e0798cc0a212e925cc5dcc66b243526c1a3  quaternius/ultimate-monsters/Big/glTF/Orc.gltf
e27e3acf826e9f5cad021c647bd3ce6f1c82d2aaa3530c03ee442df4828a710e  quaternius/ultimate-monsters/Big/glTF/Orc_Skull.gltf
ffe686b0f0e7a2fb968ea9829358f059df903d2a60c9a94bd3516b1c3d42b7c4  quaternius/ultimate-monsters/Big/glTF/Tribal.gltf
ab60f4fbc1c2e1fdc3ed4f63023dc2185240dbd55751df568099557b29af6fd5  quaternius/ultimate-monsters/Big/glTF/Yeti.gltf
```

## Made for Grubcraft (shared project folder, `assets-src/grubcraft/`)

Built in Blender on the Quaternius Farmer body and rig, so they stay out of the repo with the other Quaternius-derived files. Scripts: `design/faction-looks/ashigaru-test/source/` in the shared folder.

```
00a459d30d89f2798837eb1ec68f23d1abd1d1913f2c47c0e9017e0233cb90ff  grubcraft/yari-ashigaru/yari-ashigaru.glb
a5794578da8b93512643c5c3e7e3cb3be11f829c166ae345ba84cf23e33067fb  grubcraft/yari-ashigaru/albedo.png
af92e332e0dca7315bb677e4ae508f7692ccac6ce47056292466557efb9b37f9  grubcraft/yari-ashigaru/emissive.png
2162cc4df9a8e8345af173edcf72e074ded3818576c96217934b552ea2130f5b  grubcraft/yari-ashigaru/normal.png
33eaa7f76427af9534cedb2187d1d3d83d9a21179786c78b88bbeddbbf116f56  grubcraft/yari-ashigaru/orm.png
14379d257d7141a56e46c55cbf9631805d6300d7763269c8c2d3efbafaf5279f  grubcraft/yari-ashigaru/team.png
```

## Baked for the terrain shader (in the repo, `assets/textures/terrain/`)

`rock035_albedo.png`, `rock035_normal.png` and `rock035_rough.png` are baked at 512x512 from `ambientcg/Rock035.zip` (CC0, ambientCG): the `_Color.jpg`, `_NormalGL.jpg` (OpenGL convention) and `_Roughness.jpg` maps from the 1K JPG set, resized with Pillow LANCZOS. Albedo and normal are 8-bit RGB, roughness is 8-bit L, all saved with `optimize=True`.

```
b8f571527019ec203a4cabe83c765a719bedf65f19ab49dd3960d6d7be4a835e  rock035_albedo.png
f8a349e092a2f6079e8ad554ce41cdc512f6d42d8cfd072921d19f2d8cb36a4b  rock035_normal.png
e75ae4879124912d3b82b1c4df5628344678ad35e6f4d41c289bd889a234def2  rock035_rough.png
```

`grass001_albedo.png`, `grass001_normal.png`, `ground054_albedo.png` and `ground054_normal.png` (2026-10-23) are baked at 512x512 from `ambientcg/Grass001.zip` and `ambientcg/Ground054.zip` (CC0, ambientCG) by `tools/ground_textures/bake.py`: the `_Color.jpg` and `_NormalGL.jpg` maps of the 1K JPG sets, resized with Pillow LANCZOS (normals renormalised), saved as 8-bit RGB with `optimize=True`.

```
2f11156c686509d63e565a608f405bdbed02994f9e2641c62f194c523bb89f88  grass001_albedo.png
fbba2a3f5cd8a394a062055be96dd5f027d7e86baaeb2a8e70d2b701ebcc9b20  grass001_normal.png
b05e52f6e96412965e64eeda2865da1ab026d8e39ac38e6fbdd1f246f2e30f69  ground054_albedo.png
485b8cfed94ca8cbdfa062e5fb13458f5bb35f5679d619d08538f18cae5ddbea  ground054_normal.png
```
