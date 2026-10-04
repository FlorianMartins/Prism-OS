# Architecture de Prism OS

## Ce qu'est Prism OS

Une **couche gaming et cybersécurité posée sur un vrai Windows**, celui de l'utilisateur,
avec sa licence. Le noyau Windows reste intact : les anti-cheats voient un Windows
authentique (Secure Boot, TPM, VBS). Prism change tout ce qui est au-dessus :

- la gestion des ressources (Mode Jeu, RAM, priorités, alimentation) ;
- l'interface (lanceur, mode console, bureau modulable) — v0.2 ;
- la vie privée (télémétrie coupée par les mécanismes officiels + pare-feu) — v0.3 ;
- les profils d'usage et les outils de cybersécurité à la demande.

Prism **n'est pas** une ISO Windows modifiée, que la licence interdit de redistribuer :
c'est un logiciel qui s'applique au Windows déjà installé, et chaque réglage est réversible.

Le projet de noyau multiple from-scratch est devenu **MultiKernel**
(`FlorianMartins/MultiKernel`), un projet séparé.

## Découpage

```
crates/
  prism-core/   logique pure, sans dépendance Windows — testée sous Linux
                 config (TOML) · classement · plan · moteur · journal · RAM · outils
                 daily (Mode Quotidien) · cores (P/E, X3D) · allege · demarrage
  prism-win/    exécution sur Windows (windows-sys) : snapshot des processus,
                 application et restauration de chaque levier
  prism/        binaire prism.exe : CLI + boucle de surveillance (Mode Jeu auto)
config/
  default.toml  profils, listes protégées/compagnons/jeux, catalogue d'outils
```

La frontière clé est le trait `Platform` (`prism-core/src/platform.rs`) :
- le cœur **décide** (quoi faire, à qui, dans quel ordre, comment annuler) ;
- la plateforme **exécute** et rend la valeur d'origine pour le journal.

Toute la logique de décision est ainsi testable sans Windows, et la partie Windows est
mince, ce qui la rend auditable.

## Feuille de route

| Version | Contenu |
|---|---|
| **v0.1** ✓ | moteur Mode Jeu + politique RAM + profils + catalogue cyber, CLI, CI Windows |
| **v0.2** ✓ | Mode Quotidien permanent, cœurs P/E et X3D, services en pause, gel, surveillance RAM, applis au démarrage, réglages jeu, allègement réversible |
| v0.3 | interface : tableau de bord ressources, lanceur de jeux, mode console au démarrage ; Prism en service Windows ; tâches de démarrage des applis du Store |
| v0.4 | vie privée : télémétrie (stratégies + services + tâches + pare-feu) avec tableau de bord de ce qui est bloqué |
| v0.5 | personnalisation : thèmes, gestionnaire de fenêtres en mosaïque, barres, profils exportables |
| v0.6 | installateur, mises à jour signées, mesures FPS publiées |

## Principes

- **Léger** : Rust, pas de runtime, pas de service lourd ; le moteur dort entre deux
  passages et l'interface se ferme pendant le jeu.
- **Mesurer avant d'affirmer** : aucun « +X % de FPS » sans mesure publiée.
- **Réversible** : tout changement passe par le journal.
- **Anti-cheat d'abord** : voir `anticheat-rules.md`.
