# The facile command

| Command | Effect |
|---|---|
| `facile new NAME` | creates a whole application in the folder `NAME` |
| `facile dev [APP]` | serves, reloading at each change of a `.ry` file, with detailed errors |
| `facile serve [APP]` | serves for production |
| `facile check [APP]` | checks the whole application (Rythme's checker) without serving it |
| `facile routes [APP]` | lists the routes |
| `facile reference` | every function Facile adds to Rythme, as Markdown |
| `facile version` | the versions of Facile and Rythme |

`APP` is a folder holding `app.ry`, or a `.ry` file; by default, the current folder. Paths written in the application (`read_file`, `sqlite://app.db`, `assets`) start from its folder.

Options: `--port N`, `--workers N`, `--lang fr|en|es|de|pt|zh` (the language of the messages; `settings(app, {lang: "en"})` sets it for the application).

## Reloading

`facile dev` starts the server again as soon as a `.ry` file changes. If the application has an error, the message shows and `facile dev` waits for the next fix. Static files are read again at each request: a changed stylesheet shows by reloading the page.
