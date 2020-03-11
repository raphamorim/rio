# Rio

Rio is package manager built to use a different architecture of NPM and Yarn.
It's built over rust.

## Reason

Install dependencies in large projects is a painful job most of the times, could
be faster.

## Packages for real

```
 $ rio install ->
    - check dependencies
    - request rio.source.com for each processed dependency ->
      -> dependency exists in .rio format (gzip data + info)
        - fetch gzipped dependency (for each rio publish, it process and compress the package to .rio format)
      -> if the dependency doesn't exist:
        - process & pack the dependency asynchronously and notify the user to attemp again in an estimated time
```

## Integrity


