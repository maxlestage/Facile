## Facile contre Vitesse, à tour de rôle

Le comparatif le plus juste : les deux serveurs tournent ensemble et sont mesurés **en alternance** (`bench/ab.sh`), sept fois chacun, pour que les variations de la machine touchent les deux pareillement. On garde la médiane.

| Scénario | Facile | Vitesse | Écart |
|---|--:|--:|--:|
| `GET /` (texte) | 267 163 | 256 532 | **+4,1 %** |
| `GET /json` | 274 179 | 259 325 | **+5,7 %** |
| `GET /json`, 12 en-têtes de navigateur | 267 658 | 249 391 | **+7,3 %** |
| `GET /users/:id` (paramètre + JSON) | 276 757 | 265 733 | **+4,1 %** |
| `POST /echo` (JSON lu et renvoyé) | 250 942 | 262 727 | −4,5 % |
| `GET /` en pipeline ×16 | 2 702 136 | 2 607 071 | **+3,6 %** |

Requêtes par seconde. Facile passe devant sur cinq scénarios sur six. Sur l'écho JSON, chaque requête traverse vraiment l'interpréteur Rythme (lire le JSON en valeurs, appeler le handler, écrire le JSON) : Vitesse, en Rust compilé, garde l'avantage. À 32 connexions, l'écart sur les `GET` monte à +12 et +21 % : le moteur de Facile répond plus vite quand la machine n'est pas saturée.

## Tous les serveurs

Les mêmes scénarios, chaque serveur mesuré seul, l'un après l'autre (`bench/run.sh`), médiane de trois mesures. D'un serveur à l'autre, quelques minutes passent : les écarts de moins de 8 % entre Facile et Vitesse sont dans le bruit de la machine, d'où le tableau précédent. Les écarts avec actix-web et axum, eux, sont nets.

| Scénario | Facile | Facile sans cache | Vitesse | actix-web | axum |
|---|--:|--:|--:|--:|--:|
| `GET /` | 264 688 | 276 904 | 262 704 | 248 508 | 191 135 |
| `GET /json` | 251 993 | 274 725 | 252 254 | 249 498 | 175 985 |
| `GET /json`, navigateur | 237 712 | 250 867 | 252 333 | 191 140 | 142 658 |
| `GET /users/:id` | 267 607 | 271 150 | 272 276 | 224 220 | 166 449 |
| `POST /echo` | 233 792 | 236 361 | 256 236 | 175 884 | 129 084 |
| `GET /` pipeline ×16 | 2 638 009 | 2 128 851 | 2 682 204 | 1 274 073 | 254 865 |

## Ce que fait le cache

Sans pipeline, tous ces serveurs butent sur le même plancher : environ 5 µs de travail du noyau par requête (lire, écrire, la boucle locale). Le cache des réponses pures ne change donc presque rien au débit des requêtes isolées — il se voit dès que ce plancher est partagé : en pipeline, **2,7 millions de requêtes par seconde avec le cache, 2,1 millions sans**. Dans le processus, une réponse gardée coûte environ **73 ns** de CPU, contre environ 1 µs quand le handler Rythme s'exécute.

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
