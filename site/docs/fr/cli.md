# La commande facile

| Commande | Effet |
|---|---|
| `facile new NOM` | crée une application complète dans le dossier `NOM` |
| `facile dev [APP]` | sert avec rechargement à chaque modification d'un `.ry`, et erreurs détaillées |
| `facile serve [APP]` | sert pour la production |
| `facile check [APP]` | vérifie toute l'application (le vérificateur de Rythme) sans la servir |
| `facile routes [APP]` | liste les routes |
| `facile reference` | toutes les fonctions que Facile ajoute à Rythme, en Markdown |
| `facile version` | les versions de Facile et de Rythme |

`APP` est un dossier qui contient `app.ry`, ou un fichier `.ry` ; par défaut, le dossier actuel. Les chemins écrits dans l'application (`read_file`, `sqlite://app.db`, `assets`) partent de son dossier.

Options : `--port N`, `--workers N`, `--lang fr|en|es|de|pt|zh` (la langue des messages ; `settings(app, {lang: "fr"})` la fixe pour l'application).

## Le rechargement

`facile dev` relance le serveur dès qu'un fichier `.ry` change. Si l'application a une erreur, le message s'affiche et `facile dev` attend la correction suivante. Les fichiers statiques sont relus à chaque requête : une feuille de style modifiée se voit en rechargeant la page.
