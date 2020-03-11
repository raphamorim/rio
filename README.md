# Rio

Rio is an opinated package manager built to use a different fetch architecture of NPM or Yarn.

It's built over rust, for now Rio is recommended only for development. I strong recommend use Rio (for CI deploys: use yarn or NPM). The reason is because Rio doesn't care about integrity yet, just download speed.

## Benchmark

This test is using a lots of files (inspired by [Benchmarks of JavaScript Package Managers](https://github.com/pnpm/benchmarks-of-javascript-package-managers/blob/master/README.md))

| action  | npm | pnpm | Yarn | Rio |
| ---     | --- | --- | --- | --- |
| fresh install with over many dependencies | 6.6s | 22.4s | 35s | 2.2s |
| install with cache over many dependencies  | 2.9s	 | 1.3s | 694ms | 230ms |

## Reason

Install dependencies in large projects is a painful job most of the times and it could very be faster.

However Rio have two limitations if you try to use as NPM or yarn: It doesn't support CLI js tools (e.g: npm install -g <package>) or integrity check (Rio does a dumb check an download the hash without check the package's dependencies).

## Packages for real

```
 $ rio install ->
    - check dependencies
    - request rio.source.com for each processed dependency
      -> dependency exists in .rio format (gzip data + info)
        - fetch gzipped dependency (for each rio publish, it process and compress the package to .rio format)
      -> if the dependency doesn't exist:
        - process & pack the dependency asynchronously and notify the user to attemp again in an estimated time
```

## Universal cache

Each package is saved and it's shared across all JS projects, reducing redudance and if exists

## New package.json format

```js
 "dependencies": {
    "@babel/runtime": "https://github.com/babel/babel/tree/master/packages/babel-runtime", // always updated
    "react": "16.12.0" // load gzip package
```


## Commands && Arguments

#### `--version`

Display the Rio version.

```bash
$ rio --version
Rio 0.1.0
```

#### `--help`

Display the Rio available commands.

```bash
$ rio --version
Rio 0.1.0
Raphae Amorim <rapha850@gmail.com>
JavaScript Package Manager

USAGE:
    rio <install>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <install>    install packages
```
