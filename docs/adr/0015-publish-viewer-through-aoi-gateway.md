# 0015: ビューアを aoi-gateway 経由でも公開し、Access と cloudflared の検証に任せる

## ステータス

受理 (2026-10-12, [ADR-0013](0013-guard-viewer-sharing-the-owntracks-listener.md) の結果の一部を改める)

## 文脈

[ADR-0013](0013-guard-viewer-sharing-the-owntracks-listener.md) では、ビューアを LAN の中からだけ見られるようにし、
OwnTracks の受け口と同じ待ち受けに置くために独立したガードを重ねた。
その結果の節で、Cloudflare 以外のリバースプロキシを kgd の前に置くときと、Cloudflare Access を前に立てて
公開するときは、この決定を見直すと書いた。

ビューアを外出先のブラウザからも開きたくなった。
kgd に限らず aoi の HTTP サーバーを同じ入口から公開するため、dotfiles に aoi-gateway を作った
([設計](https://github.com/ekuinox/.dotfiles/blob/master/docs/superpowers/specs/2026-10-12-aoi-gateway-design.md))。

- Cloudflare Access のアプリ `*-aoi.ekuinox.dev` がログインを強制する
- `*.ekuinox.dev` のワイルドカード DNS が、明示したレコードの無い名前を aoi-gateway のトンネルへ向ける
- aoi の cloudflared は、全ホスト名を Caddy へ流す前に Access の JWT を検証する (Protect with Access)。JWT が無いか、署名や AUD タグが合わなければ転送しない
- aoi の Caddy は 127.0.0.1 だけで待ち受け、Host の完全一致で振り分ける。`Cf-*` と `Cdn-Loop` を削り、aoi の LAN アドレスへ転送する

ADR-0013 は、Access を前に立てるならオリジン側で JWT を検証すると想定していた。
kgd で検証すると、aoi-gateway に載せるアプリごとに同じ実装が要る。
cloudflared で検証すれば、アプリのコードを変えずに同じことができる。

## 決定

**ビューアを aoi-gateway 経由で `https://kgd-aoi.ekuinox.dev/viewer/` にも公開する。
aoi-gateway を通るリクエストは、ガード 2 と 3 に代えて、Cloudflare Access と cloudflared の JWT 検証で守る。
kgd のコードは変えない。**

aoi-gateway の経路は、ガード 2 と 3 を意図して迂回する。
Caddy が aoi の LAN アドレスへ転送するため、kgd が見る送信元は aoi の LAN アドレスになり、既定の `allowed_cidrs` に含まれる。
Caddy が `Cf-Connecting-IP`、`Cf-Ray`、`Cdn-Loop` を削るため、ガード 3 も反応しない。
kgd から見ると、aoi-gateway を通ったリクエストは LAN からのリクエストと区別がつかない。

ガード 1 と 4 は引き続き効く。
aoi-gateway から開くには、`allowed_hosts` に `kgd-aoi.ekuinox.dev` を書く。

ADR-0013 の結果にある「公開するときはオリジン側での Access の JWT 検証と合わせて設定を変える」は、
cloudflared での JWT 検証に置き換える。

## 結果

aoi-gateway の経路の信頼の境界は、Access アプリのポリシー、cloudflared の JWT 検証、Caddy の loopback での待ち受けになる。
JWT を持たないリクエストや偽の JWT は cloudflared が止め、kgd には届かない。
Access アプリを消したりホスト名を書き換えたりしても、ビューアが公開されるのではなく 403 になる。

cloudflared の検証が確かめるのは、リクエストがこの Access アプリを通ったことである。
Allow のポリシーを誤って広げた場合は、cloudflared も kgd も止められない。

aoi 上のプロセスは、127.0.0.1 の Caddy に `Host: kgd-aoi.ekuinox.dev` を付けて送れば、認証無しでビューアを読める。
ただし同じプロセスは、今日すでに aoi の LAN アドレス経由で kgd に直接届くため、露出は広がらない。

Caddy はホスト名だけで振り分けるため、`kgd-aoi.ekuinox.dev` では `/pub` と `/healthz` も Access の後ろに出る。
`/pub` は Basic 認証で守られたままである。

aoi の LAN アドレスが変わるか、本番の `allowed_cidrs` がそのアドレスを含まなくなると、aoi-gateway の経路は 403 になる。
公開される側には倒れない。

OwnTracks のトンネル (`kgd-owntracks`) は aoi-gateway とは別のトンネルで、`Cf-*` が付いたまま届く。
そのため、OwnTracks のホスト名に向けた `/viewer/` へのリクエストは、ingress の path の制限をすり抜けたとしても、今までどおりガード 2 と 3 が止める。
`just check-exposure` は OwnTracks のホスト名に対する回帰の確認として残す。
aoi-gateway 側の確認は、dotfiles の `aoi-gateway-check` で行う。
