// ============================================================================
// LEIA PRIMEIRO — como este shader funciona
// ============================================================================
//
// WGSL é a linguagem de shader do wgpu/WebGPU. Um shader não é um programa
// normal: ele roda MUITAS VEZES em paralelo, uma execução por pixel.
//
// O caminho completo do app é:
//
//   Rust (Preset)
//     -> preenche o bloco Uniforms abaixo
//     -> o "scene shader" (shine.wgsl ou gradient.wgsl) desenha a cena
//        numa textura intermediária
//     -> o "post shader" (post.wgsl) aplica correção de cor nessa textura
//     -> a textura final vira preview na tela, PNG ou frames do MP4
//
// Este arquivo é compartilhado por todos os shaders. Ele concentra:
//   - a estrutura de dados que o Rust envia para a GPU (Uniforms);
//   - o vertex shader que cobre a tela com um triângulo;
//   - funções de ruído, cor, paleta e máscaras.
//
// COORDENADAS
//   frag.xy  -> posição do pixel em pixels da textura (0..largura, 0..altura)
//   uv       -> posição normalizada (0..1, 0..1)
//   p        -> posição em coordenadas de tela: x e y entre -1 e +1,
//               corrigida pela proporção (16:10), com y positivo para cima.
//
// COR
//   Quase toda a conta é feita em "linear", que é como a luz se soma de
//   verdade. A saída é convertida para sRGB, que é como o monitor/projetor
//   espera receber os bytes. Fazer média de cores em sRGB escurece errado.
//
// DICA DE LEITURA
//   `vec2<f32>` é um par de floats; `vec3<f32>` é uma cor RGB;
//   `.x`, `.y`, `.rgb` e `.xy` são "swizzles" para pegar componentes.
//   `mix(a, b, t)` interpola linearmente: t=0 devolve a, t=1 devolve b.
//   `smoothstep(e0, e1, x)` devolve 0 antes de e0, 1 depois de e1 e uma
//   curva suave no meio. As bordas precisam estar em ordem crescente.
// ============================================================================

// ============================================================================
// CAIXA DE FERRAMENTAS MATEMÁTICAS
// ============================================================================
// As funções abaixo são nativas do WGSL. Você não precisa decorar a
// implementação, mas vale entender o que cada uma faz por dentro, porque
// quase todo shader é feito combinando essas operações.
//
// OPERAÇÕES COM VETORES
//   vec2<f32>(x, y)  -> par de floats. As contas são feitas componente a
//                       componente: (a,b) * (c,d) = (a*c, b*d).
//   v.xy, v.yzx      -> "swizzle": escolhe/reordena componentes.
//
// mix(a, b, t)
//   Fórmula: a*(1-t) + b*t.
//   É uma interpolação linear: t=0 devolve a, t=1 devolve b, t=0.5 fica no
//   meio. Também funciona com vetores, componente por componente.
//
// clamp(x, lo, hi)
//   Fórmula: min(max(x, lo), hi). Prende o valor dentro de uma faixa.
//
// floor(x) e fract(x)
//   floor(x) = maior inteiro menor ou igual a x.
//   fract(x) = x - floor(x), ou seja, só a parte fracionária (0..1).
//   Juntas, elas separam "qual célula" (floor) de "onde dentro da célula"
//   (fract). É a base do ruído e das grades.
//
// abs(x)
//   Valor absoluto: remove o sinal. Usado para medir distância sem importar
//   se está à esquerda/direita ou acima/abaixo.
//
// smoothstep(e0, e1, x)
//   Fórmula interna:
//       t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
//       resultado = t*t*(3.0 - 2.0*t);
//   Primeiro normaliza x para 0..1 entre as bordas. Depois aplica o
//   polinômio 3t²-2t³, que começa e termina com inclinação zero: a transição
//   não tem "quina". e0 precisa ser menor que e1.
//
// dot(a, b)
//   Produto escalar: a.x*b.x + a.y*b.y (+ a.z*b.z em 3D).
//   Serve para projetar um vetor em outro (Prisma) e para calcular luma
//   (média ponderada de R, G, B).
//
// length(v)
//   Fórmula: sqrt(dot(v, v)). É a distância da origem até v.
//   Para distância entre dois pontos: length(a - b).
//
// atan2(y, x)
//   Devolve o ângulo do vetor (x,y), em radianos, no intervalo -PI..PI.
//   Diferente de atan(y/x), ele olha o sinal dos dois componentes e acerta
//   o quadrante. É o que permite usar coordenadas polares.
//
// sin(t) e cos(t)
//   Descrevem um ponto andando em círculo de raio 1. Período = TAU.
//   Em t=0: sin=0, cos=1. Em t=TAU: voltam ao mesmo ponto. Por isso são
//   usados para animações que fecham o loop.
//
// pow(x, y)
//   Potência: x elevado a y. Com x=cos(...) em 0..1, um expoente maior
//   "afina" o pico (mais perto de zero em quase todo lugar).
//
// exp(x)
//   Exponencial: e elevado a x. exp(0)=1, exp(-1)≈0.37, exp(-5)≈0.0067.
//   Com exp(-d*k) você ganha um decaimento suave, sempre positivo, que
//   nunca chega exatamente a zero. É o "falloff" das luzes.
//
// select(a, b, cond)
//   Fórmula: cond ? b : a. Importante: o primeiro argumento é o valor
//   quando a condição é FALSA; o segundo é o valor quando é VERDADEIRA.
// ============================================================================

// Uniforms é o "contrato" entre Rust e GPU. O Rust monta exatamente esta
// estrutura (ver src/render/mod.rs) e a envia uma vez por frame.
struct Uniforms {
    // Tamanho da textura que está sendo desenhada, em pixels.
    // No preview é 960x600; no export é 1920x1200.
    resolution: vec2<f32>,

    // Tempo normalizado do loop: 0.0 no começo, quase 1.0 no fim.
    // O export usa frame / 360, então o vídeo de 12 s fecha o ciclo.
    time: f32,

    // "Semente" do gerador. Muda o desenho sem mudar o código.
    seed: f32,

    // Quatro controles gerais da cena.
    // x = velocidade  (slider "velocidade")
    // y = escala      (slider "escala")
    // z = intensidade (slider "intensidade")
    // w = vinheta     (slider "vinheta")
    params: vec4<f32>,

    // Controles extras.
    // x = quanto escurecer o centro atrás da letra ("escurecer centro")
    // y = índice do estilo de gradient (0..5); nos outros shaders é ignorado
    params2: vec4<f32>,

    // As 4 barras. x = estilo, y = ativa, z = defasagem (0..1), w = espelhada.
    bar_params: array<vec4<f32>, 4>,

    // 4 paletas de 4 cores, uma por barra. Índice = barra*4 + cor.
    bar_palettes: array<vec4<f32>, 16>,

    // x = barras ativas, y = gap entre barras, z = suavidade da borda.
    bars: vec4<f32>,

    // Warp global. x = ativo, y = mapa, z = força, w = escala.
    warp: vec4<f32>,
    // x = velocidade, y = seed.
    warp2: vec4<f32>,

    // Texto. O primeiro vec4 descreve a geometria:
    // x = ativa, y = altura da linha, z = largura do tile, w = proporção do tile.
    // O segundo descreve o movimento:
    // x = scroll, y = modo (0 mesmo lado, 1 alternado), z = desfasamento,
    // w = preencher tela (0/1).
    // O terceiro: x = altura entre linhas, y = direção (-1 esquerda, 1 direita).
    text_params: array<vec4<f32>, 3>,

    // Cor rgba do texto; alpha já inclui a opacidade.
    text_color: vec4<f32>,

    // Correção de cor (aplicada depois da cena, em post.wgsl).
    // x = exposição, y = brilho, z = contraste, w = saturação
    post: vec4<f32>,

    // Mais correção de cor.
    // x = temperatura, y = tint, z = gamma, w = nitidez
    post2: vec4<f32>,
};

// O grupo 0, binding 0 é o único recurso que todos os shaders de cena usam.
// O Rust cria esse buffer e o liga antes de desenhar.
@group(0) @binding(0) var<uniform> u: Uniforms;

// Barra atual. O gradient.wgsl muda isto antes de chamar um estilo; o
// shine.wgsl deixa no 0. É uma variável privada: cada pixel tem a sua.
var<private> g_bar: u32 = 0u;

// Constantes matemáticas. TAU é uma volta completa em radianos (2*PI).
const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;

// ----------------------------------------------------------------------------
// VERTEX SHADER — monta a geometria
// ----------------------------------------------------------------------------
// Um fragment shader só roda dentro de um triângulo. Em vez de criar um
// retângulo com 6 vértices, usamos um único triângulo gigante que cobre a
// tela inteira. Os vértices abaixo vão de -1 a +3 em clip space; o pedaço
// que interessa é só o que cai dentro de [-1, +1]. Isso é um truque clássico
// chamado "fullscreen triangle".
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    // O terceiro componente é z (profundidade) e o quarto é w (divisão
    // perspectiva). z=0 e w=1 mantêm o triângulo na frente e sem distorção.
    return vec4<f32>(positions[index], 0.0, 1.0);
}

// ----------------------------------------------------------------------------
// COORDENADAS
// ----------------------------------------------------------------------------
// Converte a posição do pixel (0..largura, 0..altura) para o sistema que as
// cenas usam:
//   1. divide pela resolução -> uv em 0..1;
//   2. multiplica por 2 e subtrai 1 -> NDC em -1..+1;
//   3. multiplica x pela proporção para círculos não virarem ovais;
//   4. inverte y para que "para cima" seja positivo (na tela, y cresce para
//      baixo; na matemática, normalmente cresce para cima).
fn screen_p(frag: vec2<f32>) -> vec2<f32> {
    let uv = frag / u.resolution;
    let p = uv * 2.0 - 1.0;
    return vec2<f32>(p.x * (u.resolution.x / u.resolution.y), -p.y);
}

// ----------------------------------------------------------------------------
// TEMPO COM LOOP SEM EMENDA
// ----------------------------------------------------------------------------
// `speed` não pode simplesmente multiplicar o tempo: em t=1 o desenho teria
// uma fase diferente de t=0 e o loop daria um "pulo". Esta função deforma o
// tempo com uma senoide que vale zero em t=0 e em t=1. Assim a velocidade
// local muda, mas o começo e o fim continuam idênticos.
fn loop_time(t: f32, speed: f32) -> f32 {
    return t + ((speed - 1.0) * sin(TAU * t) / TAU);
}

// ----------------------------------------------------------------------------
// RUÍDO DETERMINÍSTICO
// ----------------------------------------------------------------------------
// hash21 transforma duas coordenadas num número pseudoaleatório entre 0 e 1.
// Não existe estado nem tabela: a mesma entrada sempre gera a mesma saída.
// É por isso que o preview e o export produzem exatamente o mesmo frame.
//
// Passo a passo:
//   1. monta um vec3 com (x, y, x) e multiplica por 0.1031. O 0.1031 é uma
//      constante arbitrária escolhida para espalhar os valores; não é mágica.
//   2. fract() mantém tudo em 0..1 e mistura as casas decimais.
//   3. p3.yzx gira os componentes. O dot() combina os três num único número,
//      somando produtos. O +33.33 evita que valores pequenos fiquem presos
//      perto de zero.
//   4. fract((x+y)*z) faz a mistura final e devolve 0..1.
// Não é um hash criptográfico; é só "aleatório o suficiente" para desenhar.
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// noise21 é um ruído suave: em vez de sortear cada pixel, sorteia os cantos
// de uma célula e interpola entre eles.
//
// Passo a passo:
//   i = floor(p) -> qual célula (número inteiro).
//   f = fract(p) -> onde estamos dentro da célula (0..1).
//   w = f*f*(3-2f) -> peso com inclinação zero nas bordas, para a transição
//        entre células não ter quinas.
//   a, b, c, d -> hash dos quatro cantos da célula.
//   mix(mix(a,b,wx), mix(c,d,wx), wy) -> interpolação bilinear:
//        primeiro em x (a->b e c->d), depois em y entre os dois resultados.
// O ruído em si é estático; quem anima é o chamador, deslocando p com
// sin(phase)/cos(phase).
fn noise21(p: vec2<f32>) -> f32 {
    let i = floor(p); // canto inferior-esquerdo da célula
    let f = fract(p); // posição dentro da célula, 0..1
    let w = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    // Primeiro interpola em x, depois em y (bilinear).
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

// fbm = "fractal Brownian motion". Soma várias camadas de ruído:
// cada camada é menor em amplitude e maior em frequência. O resultado
// parece uma nuvem/fumaça com detalhe em várias escalas.
//
// A conta é aproximadamente:
//   0.500 * noise(p)
// + 0.250 * noise(2.03*p + offset)
// + 0.125 * noise(4.12*p + outro_offset)
// ...
// Multiplicar p por 2.03 dobra a frequência (mais detalhe). Multiplicar a
// amplitude por 0.5 faz cada camada pesar menos. O offset evita que todas as
// camadas fiquem alinhadas na origem e criem um padrão repetido.
fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var point = p;
    for (var i = 0; i < 3; i += 1) {
        value += noise21(point) * amplitude;
        point = point * 2.03 + vec2<f32>(17.1, 9.7);
        amplitude *= 0.5;
    }
    return value;
}

// Versão com 5 oitavas: soma mais camadas, então tem detalhe mais fino e
// transições ainda mais suaves. Usada na Névoa.
fn fbm5(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var point = p;
    for (var i = 0; i < 5; i += 1) {
        value += noise21(point) * amplitude;
        point = point * 2.03 + vec2<f32>(17.1, 9.7);
        amplitude *= 0.5;
    }
    return value;
}

// ----------------------------------------------------------------------------
// ESPAÇOS DE COR
// ----------------------------------------------------------------------------
// sRGB é a curva usada por monitores/projetores. Ela não é linear: valores
// baixos recebem mais precisão e valores altos são comprimidos. Para somar
// luz, misturar cores e aplicar exposição corretamente, primeiro decodificamos
// para linear. A fórmula é a padrão da indústria, com um trecho linear perto
// do preto:
//
//   se c <= 0.04045:  linear = c / 12.92
//   senão:            linear = ((c + 0.055) / 1.055) ^ 2.4
//
// O trecho linear existe porque a potência 2.4 tem inclinação infinita em
// zero; sem ele, pretos quase iguais virariam diferenças enormes. O 12.92 é
// escolhido para as duas partes se encontrarem no mesmo ponto (0.04045 sRGB
// vira 0.0031308 linear).
//
// `select(a, b, cond)` devolve `a` quando cond é falso e `b` quando é true.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// Caminho inverso: linear -> sRGB. É a última conta antes de gravar os bytes.
//   se c <= 0.0031308:  sRGB = c * 12.92
//   senão:              sRGB = 1.055 * c^(1/2.4) - 0.055
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let low = c * 12.92;
    let high = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, c <= vec3<f32>(0.0031308));
}

// OKLab é um espaço de cor "perceptual": distâncias nele se parecem com a
// diferença que o olho percebe. Interpolar duas cores em RGB costuma passar
// por um cinza sujo; em OKLab o caminho fica mais natural.
//
// A conversão é uma sequência de multiplicações de matriz. Cada linha é um
// dot product: por exemplo, o primeiro valor é
//   0.4122*r + 0.5363*g + 0.0514*b.
// Isso transforma RGB (as três cores do monitor) em LMS, que representa a
// resposta dos três tipos de cone do olho de forma simplificada.
fn linear_to_oklab(c: vec3<f32>) -> vec3<f32> {
    // RGB linear -> LMS. As linhas da matriz são os pesos de cada canal.
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    // A raiz cúbica aproxima a resposta não linear do olho. max(...,0)
    // evita raiz de número negativo (que viraria NaN).
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    // LMS -> OKLab: x = luminosidade, y = verde/vermelho, z = azul/amarelo.
    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    );
}

// OKLab -> RGB linear. É a matriz inversa da função acima: primeiro volta
// de Lab para LMS (l_, m_, s_), depois desfaz a raiz cúbica elevando ao
// cubo (l = l_*l_*l_) e, por fim, aplica a matriz LMS -> RGB.
fn oklab_to_linear(lab: vec3<f32>) -> vec3<f32> {
    let l_ = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    let m_ = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    let s_ = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    // Desfaz a raiz cúbica.
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    return vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    );
}

// ----------------------------------------------------------------------------
// PALETA
// ----------------------------------------------------------------------------
// Recebe t em 0..1 e devolve uma cor da paleta de 4 cores.
// A paleta é dividida em 3 segmentos: [0]->[1], [1]->[2], [2]->[3].
//
// Exemplo com t = 0.5:
//   x = 0.5 * 3 = 1.5
//   i = floor(1.5) = 1          -> segmento entre palette[1] e palette[2]
//   f = 1.5 - 1 = 0.5           -> metade do caminho
//   resultado = mix(palette[1], palette[2], 0.5)
//
// O min(..., 2) impede i = 3 quando t = 1.0 (aí usamos o segmento
// palette[2] -> palette[3] com f = 1, que devolve palette[3]).
fn palette_linear(t: f32) -> vec3<f32> {
    let x = clamp(t, 0.0, 1.0) * 3.0;
    let i = min(u32(floor(x)), 2u);
    let f = x - floor(x);
    let a = srgb_to_linear(u.bar_palettes[g_bar * 4u + i].rgb);
    let b = srgb_to_linear(u.bar_palettes[g_bar * 4u + i + 1u].rgb);
    // smoothstep(0,1,f) suaviza a entrada/saída de cada segmento. Sem ele,
    // a derivada mudaria de repente na fronteira entre segmentos.
    let lab = mix(linear_to_oklab(a), linear_to_oklab(b), smoothstep(0.0, 1.0, f));
    return max(oklab_to_linear(lab), vec3<f32>(0.0));
}

// ----------------------------------------------------------------------------
// MÁSCARAS
// ----------------------------------------------------------------------------
// Aplica dois efeitos que todas as cenas compartilham:
//   1. vinheta: escurece as bordas para o olho ir ao centro;
//   2. zona de texto: escurece uma faixa horizontal no meio, onde a letra
//      do Holyrics vai ficar. Só escurece onde a cor já é clara (luma alta),
//      para não criar uma mancha visível no fundo escuro.
fn apply_masks(col: vec3<f32>, p: vec2<f32>) -> vec3<f32> {
    // length(p) = sqrt(x² + y²): distância do centro da tela até o pixel.
    // Nas bordas laterais chega a ~1.6; nos cantos, ~1.88.
    let radius = length(p);
    // 1 no centro, 0 nas bordas. A transição começa em 0.35 e termina em 1.9.
    let vignette = 1.0 - smoothstep(0.35, 1.9, radius);
    // mix(1, vignette, força): força 0 desliga; força 1 aplica inteira.
    var c = col * mix(1.0, vignette, u.params.w);

    // Faixa horizontal: abs(p.y + 0.02) mede a distância vertical do centro.
    // Dividir por 0.34 normaliza a altura da faixa. O smoothstep devolve 1
    // perto do centro e 0 quando a distância normalizada passa de 0.62
    // (aproximadamente |p.y| > 0.21).
    let zone = 1.0 - smoothstep(0.12, 0.62, abs(p.y + 0.02) / 0.34);

    // Luma = brilho percebido. É um dot product com os pesos da Rec. 709:
    //   0.2126*R + 0.7152*G + 0.0722*B.
    // O verde pesa mais porque o olho é mais sensível a ele.
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));

    // Escurece proporcionalmente à luma: fundo escuro quase não muda;
    // destaque claro atrás da letra escurece. O 0.85 limita a força máxima.
    let amount = u.params2.x * zone * smoothstep(0.02, 0.35, luma) * 0.85;
    // c *= (1 - amount) é o mesmo que mix(c, preto, amount).
    c *= 1.0 - amount;
    return c;
}

// ----------------------------------------------------------------------------
// SAÍDA DA CENA
// ----------------------------------------------------------------------------
// Tone mapping: `1 - exp(-x*k)` comprime os realces com suavidade.
//   x = 0      -> 1 - 1 = 0
//   x grande   -> 1 - 0 = 1 (nunca passa de 1)
//   inclinação em x=0 = k (aqui 1.15)
// Ou seja: valores baixos quase não mudam; valores altos se aproximam de 1
// sem criar um corte branco. Depois converte para sRGB e garante alpha 1.0.
// O dither final é aplicado no post.wgsl, não aqui.
fn finish(linear: vec3<f32>) -> vec4<f32> {
    let toned = vec3<f32>(1.0) - exp(-max(linear, vec3<f32>(0.0)) * 1.15);
    let srgb = linear_to_srgb(toned);
    return vec4<f32>(clamp(srgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
