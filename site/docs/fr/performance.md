## Facile contre Vitesse, à tour de rôle

Le comparatif le plus juste : les deux serveurs tournent ensemble et sont mesurés **en alternance** (`bench/ab.sh`), sept fois chacun, pour que les variations de la machine touchent les deux pareillement. On garde la médiane.

| Scénario | Facile | Vitesse | Écart |
|---|--:|--:|--:|
| `GET /` (texte) | 182 211 | 176 846 | +3,0 % |
| `GET /json` | 185 985 | 184 331 | +0,9 % |
| `GET /json`, 12 en-têtes de navigateur | 165 742 | 172 452 | −3,9 % |
| `GET /users/:id` (paramètre + JSON) | 193 260 | 184 753 | **+4,6 %** |
| `POST /echo` (JSON lu et renvoyé) | 151 567 | 179 761 | −15,7 % |
| `GET /` en pipeline ×16 | 1 834 231 | 1 817 494 | +0,9 % |

Requêtes par seconde. Facile et Vitesse sont au coude-à-coude : depuis novembre 2026, Vitesse a un moteur par thread comme celui de Facile (un epoll par cœur, pas de tâche par connexion, une courte attente active avant de dormir), et les écarts sur quatre scénarios restent dans le bruit de la machine (5 %). Facile passe devant sur `/users/:id` grâce à son cache des réponses pures (la réponse est gardée, le handler ne tourne plus) ; Vitesse passe devant avec la requête d'un navigateur, et nettement sur l'écho JSON, où chaque requête traverse vraiment l'interpréteur Rythme (lire le JSON en valeurs, appeler le handler, écrire le JSON).

## Tous les serveurs

Les mêmes scénarios, chaque serveur mesuré seul, l'un après l'autre (`bench/run.sh`), médiane de trois mesures. D'un serveur à l'autre, quelques minutes passent : les écarts de moins de 8 % entre Facile et Vitesse sont dans le bruit de la machine, d'où le tableau précédent. Les écarts avec actix-web et axum, eux, sont nets.

| Scénario | Facile | Facile sans cache | Vitesse | actix-web | axum |
|---|--:|--:|--:|--:|--:|
| `GET /` | 179 063 | 180 142 | 175 956 | 174 475 | 104 439 |
| `GET /json` | 180 760 | 174 481 | 173 191 | 152 498 | 101 206 |
| `GET /json`, navigateur | 174 346 | 153 930 | 165 337 | 138 917 | 86 681 |
| `GET /users/:id` | 192 112 | 166 986 | 178 731 | 152 877 | 96 321 |
| `POST /echo` | 142 811 | 155 466 | 171 438 | 115 069 | 81 171 |
| `GET /` pipeline ×16 | 1 977 558 | 1 300 910 | 1 770 173 | 983 771 | 171 985 |

Mesuré le 3 novembre 2026, dans la même session que le tableau précédent, sur une machine virtuelle plus lente que celle des mesures précédentes (comparez les colonnes, pas avec d'anciens chiffres). Sans son cache, Facile exécute chaque handler dans l'interpréteur Rythme : Vitesse, en Rust compilé, passe alors devant sur quatre scénarios sur six.

## Ce que fait le cache

Sans pipeline, tous ces serveurs butent sur le même plancher : environ 5 µs de travail du noyau par requête (lire, écrire, la boucle locale). Le cache des réponses pures ne change donc presque rien au débit des requêtes isolées — il se voit dès que ce plancher est partagé : en pipeline, **2,0 millions de requêtes par seconde avec le cache, 1,3 million sans**. Dans le processus, une réponse gardée coûte environ **73 ns** de CPU, contre environ 1 µs quand le handler Rythme s'exécute.

## La méthode

- Machine virtuelle de 4 vCPU ; le serveur est épinglé sur 2 cœurs (`taskset -c 0,1`), wrk sur les 2 autres ; 128 connexions keep-alive ; 4 à 5 secondes par mesure.
- Les serveurs de Vitesse, actix-web et axum sont ceux du dossier `bench/` de Vitesse ; ceux de Facile sont les mêmes routes écrites en Rythme (`bench/app.ry`).
- Le temps CPU du serveur est lu dans `/proc` pendant la mesure.
- Pour reproduire : `bench/run.sh` (tous) et `bench/ab.sh` (Facile contre Vitesse, en alternance).

## Pourquoi c'est rapide

- **Un thread par cœur**, chacun avec sa boucle d'événements (epoll), sa socket (`SO_REUSEPORT`) et son interpréteur Rythme : aucun verrou, aucune requête ne change de thread.
- **Pas d'allocation** pour lire une requête : la tête est découpée par httparse (SIMD), les en-têtes restent des positions dans le tampon.
- **Une écriture par lot** : les réponses aux requêtes pipelinées partent ensemble.
- **Une attente courte** avant de dormir : après un tour chargé, la boucle regarde encore quelques microsecondes s'il arrive du travail, ce qui coûte moins que réveiller un thread endormi.
- **L'horloge lue une fois par tour**, l'en-tête `Date` écrit une fois par seconde.
- **mimalloc**, et des valeurs Rythme construites sans détour (JSON lu directement en enregistrements, formes partagées).
