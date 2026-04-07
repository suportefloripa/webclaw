Para subir a API com Docker e exigir o token secreto, use este fluxo na raiz do projeto:

  docker build -t webclaw .

  # O mais simples (proxies.txt incluído no build se existir):
  docker run -d --name webclaw-api -p 3000:3000 -e PORT=3000 -e API_KEY=secreto webclaw webclaw-api

  # Com volume manual (para atualizar proxies sem rebuild):
  docker run -d --name webclaw-api -p 3000:3000 -e PORT=3000 -e API_KEY=secreto -v /home/user/Dev/webclaw/proxies.txt:/app/proxies.txt webclaw webclaw-api

  Depois, chame a API assim:
curl -X POST http://localhost:3000/api/scrape \
     -H "Authorization: Bearer secreto" \
     -H "Content-Type: application/json" \
     -d '{"url": "https://api.ipify.org/"}'

----

  curl -X POST http://localhost:3000/api/scrape \
    -H "Authorization: Bearer secreto" \
    -H "Content-Type: application/json" \
    -d '{"url":"https://cnpja.com/office/07526557011659"}'

  O motivo do webclaw-api no fim do docker run é que o Dockerfile inicia o CLI por padrão, não a API: Dockerfile. A autenticação usa a
  variável API_KEY e espera Authorization: Bearer <token>: crates/webclaw-api/src/main.rs. Se quiser, eu também posso te passar uma
  versão pronta em docker compose para rodar a API com esse token.

  ----

docker stop webclaw-api && docker rm webclaw-api

  o stop é:

  docker stop webclaw-api

  Se quiser remover o container depois:

  docker rm webclaw-api

  Se estiver usando docker compose, aí seria:

  docker compose down



---------------------------------------------------------------
  1. Adicionado suporte a proxy pool - mesma lógica do webclaw-mcp
  2. Criado AppState com cliente compartilhado (melhor performance)
  3. Variáveis de ambiente suportadas:
    - WEBCLAW_PROXY - proxy único
    - WEBCLAW_PROXY_FILE - arquivo de pool (default: proxies.txt)

  DOKPLOY.md

  Adicionada seção de configuração de proxy.

  ---
  Como usar o Docker (Agora automático se proxies.txt estiver na raíz no build)

  docker run -d --name webclaw-api \
    -p 3000:3000 \
    -e PORT=3000 \
    -e API_KEY=secreto \
    webclaw webclaw-api

  Se preferir usar volume:
  docker run -d --name webclaw-api \
    -p 3000:3000 \
    -e PORT=3000 \
    -e API_KEY=secreto \
    -v $(pwd)/proxies.txt:/app/proxies.txt \
    webclaw webclaw-api

  No startup você verá: INFO webclaw-api: loaded proxy pool count=1 file=proxies.txt

