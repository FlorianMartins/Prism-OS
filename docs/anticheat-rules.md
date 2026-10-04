# Règles de compatibilité anti-cheat (non négociables)

Prism doit rester, pour un anti-cheat, **un Windows ordinaire**. Toute contribution qui
enfreint une de ces règles est refusée.

1. **Ne jamais toucher au processus du jeu** : pas d'injection, pas de lecture ni
   d'écriture de sa mémoire, pas de handle ouvert sur lui au-delà de la lecture de son
   nom et de sa date de création.
2. **Ne jamais toucher aux processus d'anti-cheat** (`vgc`, `vgtray`, `EasyAntiCheat*`,
   `BEService*`, `FACEIT*`, `EAAntiCheat*`…), d'antivirus, ni aux processus système.
   La liste vit dans `config/default.toml` (`protected`).
3. **Pas de pilote noyau Prism.** Tout passe par des API user-mode documentées.
4. **Ne jamais désactiver une protection exigée par un anti-cheat** : Secure Boot, TPM,
   VBS/HVCI, Defender, signature des pilotes, mode test désactivé. Si un jour une option
   « performances » touche à VBS, elle sera explicite, avertie et réversible, jamais par
   défaut.
5. **Ne modifier aucun fichier système signé.** Les réglages passent par les API, les
   stratégies et la configuration officielles.
6. **Windows Update reste fonctionnel** : les anti-cheats exigent des versions à jour.
7. **Signaler, ne pas masquer** : un outil gênant (débogueur, pilote noyau tiers) est
   signalé à l'utilisateur, jamais caché à l'anti-cheat.
