# Prism OS

**La couche gaming et cybersécurité pour un vrai Windows.**

Prism s'installe sur *votre* Windows, avec votre licence, et laisse son noyau intact. Les
anti-cheats (Vanguard, FACEIT, EAC, BattlEye) voient un Windows authentique. Prism
reprend la main sur tout ce qui est au-dessus : les ressources, les profils d'usage, les
outils de sécurité et, bientôt, l'interface et la vie privée.

> Le projet de noyau multiple from-scratch qui portait ce nom s'appelle désormais
> [MultiKernel](https://github.com/FlorianMartins/MultiKernel).

## Ce que fait Prism

- **Mode Quotidien, tout le temps** : une appli d'arrière-plan inactive passe sur les
  cœurs économes en basse consommation, puis rend sa RAM. Dès que vous y revenez, elle
  retrouve tout. Windows devient plus léger même sans jouer.
- **Applis au démarrage** (`prism demarrage`) : conseils et désactivation réversible,
  comme le Gestionnaire des tâches. Mesuré : −1,2 Go et −26 processus sur un PC avec
  Discord, Steam, OneDrive et Edge ([mesures](docs/mesures.md)).
- **Mode Jeu automatique** : dès qu'un jeu démarre (Steam, Epic, GOG, Xbox, Riot, EA,
  Ubisoft, Battle.net), l'arrière-plan passe en retrait (priorité, EcoQoS, cœurs
  économes) et rend sa RAM. À la fermeture, tout revient à l'identique.
- **RAM libérée intelligemment** : la mémoire de Word ou de Chrome, ouverts mais
  inactifs, est réellement rendue au jeu, **sans** vider le cache où vivent les données
  du jeu. Explication complète : [spec §4](docs/specs/v0.1-mode-jeu.md#4-politique-ram--pourquoi-cet-ordre-précis).
- **Gestion fine en jeu** : arrière-plan sur les cœurs E (Intel hybride) ou sur la
  puce sans cache 3D (Ryzen X3D) sans toucher au jeu, indexation en pause, gel des applis
  choisies, surveillance de la RAM en pleine partie.
- **Réglages jeu** (`prism allege apply jeu`) : Mode Jeu de Windows, planification GPU
  matérielle, jeux en fenêtre, accélération souris.
- **Profils** : Gaming, Équilibré, Cyber.
- **Allègement de Windows réversible** (`prism allege`) : télémétrie, services
  inutiles, Edge en arrière-plan… avec 72 services protégés que les anti-cheats
  exigent. Gains mesurés et honnêtes : [docs/mesures.md](docs/mesures.md).
- **Outils cyber à la demande** : Wireshark, Burp, ZAP, Sysinternals, x64dbg, Kali sous
  WSL (nmap, sqlmap, hashcat…). Rien n'est installé par défaut, rien ne tourne pendant
  le jeu, et les outils qui gênent les anti-cheats (débogueurs) sont signalés.
- **Compatible anti-cheat par construction** : aucun pilote, aucune injection, aucun
  fichier système modifié. Voir les [règles](docs/anticheat-rules.md).
- **Sûr** : chaque changement est journalisé avant le suivant, et un arrêt brutal est
  réparé au démarrage suivant.

## Essayer

```powershell
prism demo                 # une partie simulée, de bout en bout
prism status               # ce que Prism voit : profil, RAM, jeux, conflits
prism profile gaming       # ou balanced, cyber
prism watch                # Mode Quotidien + Mode Jeu automatique (Ctrl-C restaure tout)
prism top                  # qui consomme le processeur et la RAM
prism demarrage            # applis au démarrage ; demarrage recommande / off <nom> / restore
prism ram clean            # libère la RAM de l'arrière-plan maintenant
prism allege apply         # allègement sûr (avance : plus poussé) ; allege restore annule
prism tools                # packs d'outils cyber
prism autostart on         # au démarrage de la session (console administrateur)
```

Exemple (`prism demo`) :

```
> Lancement de VALORANT
RAM : 1.4 Go libre sur 16.0 Go (8 %) · cache 4.0 Go dont 0 Mo en priorité basse

Mode Jeu activé pour valorant-win64-shipping.exe :
  3 processus passés en retrait
  3 processus en EcoQoS (cœurs économes)
  3 priorités mémoire changées
  3 mémoires de travail rognées
  purge du cache en attente (Low)
  plan d'alimentation -> performances élevées
  arrêt de WSL (libère vmmem)
  RAM libérée : 6.0 Go
```

## Construire

```bash
cargo test --workspace                                        # partout
cargo build --release -p prism --target x86_64-pc-windows-gnu # prism.exe depuis Linux (mingw-w64)
```

Sous Windows : `cargo build --release -p prism`.

## Structure

| Dossier | Rôle |
|---|---|
| `crates/prism-core` | décisions : classement, plan, politique RAM, journal, outils — testé sans Windows |
| `crates/prism-win` | exécution sur Windows (API documentées, `windows-sys`) |
| `crates/prism` | `prism.exe` : ligne de commande et surveillance |
| `config/default.toml` | profils, listes, catalogue d'outils |
| `docs/` | [architecture](docs/ARCHITECTURE.md), [spec v0.1](docs/specs/v0.1-mode-jeu.md), [spec v0.2](docs/specs/v0.2-processus.md), [règles anti-cheat](docs/anticheat-rules.md), [mesures](docs/mesures.md) |

## Feuille de route

Mesures publiées : [docs/mesures.md](docs/mesures.md).

v0.2 moteur ✓ (Mode Quotidien, cœurs P/E et X3D, démarrage, réglages jeu) · v0.3
interface (tableau de bord, lanceur, mode console) et service Windows · v0.4 vie
privée (télémétrie, debloat réversible) · v0.5 personnalisation · v0.6 installateur et
mesures FPS publiées. Détails : [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Licence

Prism OS est un logiciel libre, au choix sous l'une de ces deux licences :

- [MIT](LICENSE-MIT)
- [Apache 2.0](LICENSE-APACHE)

Sauf mention contraire, toute contribution envoyée au projet est publiée sous ces
deux mêmes licences, sans condition supplémentaire.
