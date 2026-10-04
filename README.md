# Prism OS

**La couche gaming et cybersécurité pour un vrai Windows.**

Prism s'installe sur *votre* Windows, avec votre licence, et laisse son noyau intact. Les
anti-cheats (Vanguard, FACEIT, EAC, BattlEye) voient un Windows authentique. Prism
reprend la main sur tout ce qui est au-dessus : les ressources, les profils d'usage, les
outils de sécurité et, bientôt, l'interface et la vie privée.

> Le projet de noyau multiple from-scratch qui portait ce nom s'appelle désormais
> [MultiKernel](https://github.com/FlorianMartins/MultiKernel).

## Ce que fait la v0.1

- **Mode Jeu automatique** : dès qu'un jeu démarre (Steam, Epic, GOG, Xbox, Riot, EA,
  Ubisoft, Battle.net), l'arrière-plan passe en retrait (priorité, EcoQoS, cœurs
  économes) et rend sa RAM. À la fermeture, tout revient à l'identique.
- **RAM libérée intelligemment** : la mémoire de Word ou de Chrome, ouverts mais
  inactifs, est réellement rendue au jeu, **sans** vider le cache où vivent les données
  du jeu. Explication complète : [spec §4](docs/specs/v0.1-mode-jeu.md#4-politique-ram--pourquoi-cet-ordre-précis).
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
prism watch                # Mode Jeu automatique (Ctrl-C restaure tout)
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
| `docs/` | [architecture](docs/ARCHITECTURE.md), [spec v0.1](docs/specs/v0.1-mode-jeu.md), [règles anti-cheat](docs/anticheat-rules.md) |

## Feuille de route

Mesures publiées : [docs/mesures.md](docs/mesures.md).

v0.2 interface (tableau de bord, lanceur, mode console, cœurs P/E et X3D) · v0.3 vie
privée (télémétrie, debloat réversible) · v0.4 personnalisation · v0.5 installateur et
mesures FPS publiées. Détails : [ARCHITECTURE.md](docs/ARCHITECTURE.md).
