
* [hchalouati](https://github.com/hchalouati)
* [medazizktata25](https://github.com/medazizktata25)
* [aymennasri](https://github.com/aymennasri)
* [YGhorbel](https://github.com/YGhorbel)
* [mdp](https://github.com/mdp)
* [sirinebenyedder](https://github.com/sirinebenyedder)
* [WWKoch](https://github.com/WWKoch)

```shell
p=1;
while true; do
    s=$(curl "https://api.github.com/repos/Guepard-Corp/gfs/contributors?page=$p") || break
    [ "0" = $(echo $s | jq length) ] && break
    echo $s | jq -r '.[] | "* [" + .login + "](" + .html_url + ")"'
    p=$((p+1))
done
```