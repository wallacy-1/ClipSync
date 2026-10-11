package app.clipsync.desktop.ime

import android.content.Context
import android.view.textservice.SentenceSuggestionsInfo
import android.view.textservice.SpellCheckerSession
import android.view.textservice.SuggestionsInfo
import android.view.textservice.TextInfo
import android.view.textservice.TextServicesManager
import java.text.Normalizer
import java.util.Locale

/**
 * Sugestões e autocorreção pelo corretor ortográfico do sistema, sem dicionário próprio. Nada é guardado.
 *
 * O corretor pode não ter o dicionário do idioma do teclado e aí aceita qualquer palavra ou responde
 * em outro idioma. Cada sessão testa dois canários (uma palavra comum e uma inventada); se errar
 * qualquer um, as sugestões ficam desligadas nesse idioma.
 */
class Suggester(context: Context, private val onResult: (Result) -> Unit) : SpellCheckerSession.SpellCheckerSessionListener {

    /**
     * [word] é a palavra consultada; [correct] = está no dicionário; [correction] = troca que o
     * corretor recomenda com confiança e que passou pelos filtros (candidata a autocorreção).
     */
    class Result(val word: String, val suggestions: List<String>, val correct: Boolean, val correction: String?)

    private val tsm = context.getSystemService(Context.TEXT_SERVICES_MANAGER_SERVICE) as? TextServicesManager
    private var session: SpellCheckerSession? = null
    private var locale: Locale? = null
    /** O corretor conhece o idioma atual (respondeu a palavra-canário). */
    private var supported = false
    private var canaryOk = 0
    private var probedAt = 0L
    private var probes = 0
    /** Palavras pedidas e ainda sem resposta; o corretor responde na ordem dos pedidos. */
    private val inflight = ArrayDeque<String>()
    private var latest: String? = null

    fun setLocale(loc: Locale) {
        val now = android.os.SystemClock.uptimeMillis()
        if (loc == locale && session != null) {
            // O corretor carrega o dicionário sob demanda: o primeiro teste pode falhar à toa.
            if (!supported && probes < MAX_PROBES && now - probedAt > REPROBE_MS) probe(session!!, loc, now)
            return
        }
        session?.close()
        locale = loc
        inflight.clear()
        latest = null
        // false: usa o idioma do teclado, não o configurado no corretor.
        val s = runCatching { tsm?.newSpellCheckerSession(null, loc, this, false) }.getOrNull()
        session = s
        supported = false
        probes = 0
        if (s != null) probe(s, loc, now)
    }

    private fun probe(s: SpellCheckerSession, loc: Locale, now: Long) {
        probedAt = now
        probes++
        canaryOk = 0
        inflight.addLast(CANARY_KEY)
        inflight.addLast(NONSENSE_KEY)
        runCatching {
            s.getSentenceSuggestions(arrayOf(TextInfo(CANARY[loc.language] ?: NONSENSE)), 1)
            s.getSentenceSuggestions(arrayOf(TextInfo(NONSENSE)), 1)
        }.onFailure { inflight.clear() }
    }

    fun request(word: String) {
        val s = session ?: return
        if (!supported) {
            // O corretor responde errado enquanto carrega o dicionário: testa de novo enquanto a pessoa digita.
            val now = android.os.SystemClock.uptimeMillis()
            val loc = locale
            if (loc != null && probes < MAX_PROBES && now - probedAt > REPROBE_MS) probe(s, loc, now)
            return
        }
        latest = word
        if (inflight.size > 8) inflight.clear()
        inflight.addLast(word)
        runCatching { s.getSentenceSuggestions(arrayOf(TextInfo(word)), MAX) }.onFailure { inflight.removeLastOrNull() }
    }

    fun cancel() {
        latest = null
    }

    fun close() {
        session?.close()
        session = null
    }

    override fun onGetSentenceSuggestions(results: Array<out SentenceSuggestionsInfo>?) {
        val word = inflight.removeFirstOrNull() ?: return
        val info = results?.firstOrNull()?.takeIf { it.suggestionsCount > 0 }?.getSuggestionsInfoAt(0)
        val attrs = info?.suggestionsAttributes ?: 0
        val correct = attrs and SuggestionsInfo.RESULT_ATTR_IN_THE_DICTIONARY != 0
        if (word == CANARY_KEY || word == NONSENSE_KEY) {
            val ok = if (word == CANARY_KEY) correct || CANARY[locale?.language] == null else !correct
            if (ok) canaryOk++
            supported = canaryOk == 2
            if (word == NONSENSE_KEY) ClipboardBridge.debug("spell checker language=${locale?.language} supported=$supported")
            return
        }
        if (word != latest || info == null) return // a palavra já mudou; vale a resposta do pedido mais novo
        val typo = attrs and SuggestionsInfo.RESULT_ATTR_LOOKS_LIKE_TYPO != 0
        val recommended = attrs and SuggestionsInfo.RESULT_ATTR_HAS_RECOMMENDED_SUGGESTIONS != 0
        val raw = (0 until info.suggestionsCount.coerceAtLeast(0))
            .map { info.getSuggestionAt(it) }
            // "am Anaheim": o corretor também sugere separar a palavra; aqui isso só atrapalha.
            .filter { it.isNotBlank() && ' ' !in it && it != word && close(word, it) }
        val list = raw.map { matchCase(word, it) }.distinct()
        // Acento esquecido ganha de qualquer outra troca, mesmo que o corretor a ponha mais abaixo.
        val accentFix = raw.firstOrNull { fold(it) == fold(word) }
        val correction = when {
            correct -> null
            accentFix != null && safeToAutocorrect(word, accentFix) -> accentFix
            typo && recommended -> raw.firstOrNull()?.takeIf { safeToAutocorrect(word, it) }
            else -> null
        }?.let { matchCase(word, it) }
        onResult(Result(word, list, correct, correction))
    }

    override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) {}

    /** "Casa" → "Casamento", "CASA" → "CASAMENTO": a sugestão segue a caixa do que foi digitado. */
    private fun matchCase(typed: String, s: String): String = when {
        typed.length > 1 && typed.all { !it.isLetter() || it.isUpperCase() } -> s.uppercase(locale ?: Locale.getDefault())
        typed.firstOrNull()?.isUpperCase() == true -> s.replaceFirstChar { it.titlecase(locale ?: Locale.getDefault()) }
        else -> s
    }

    /** Sugestão parecida o bastante com o que foi digitado para valer mostrar (completar conta). */
    private fun close(typed: String, s: String): Boolean {
        val a = fold(typed)
        val b = fold(s)
        return b.startsWith(a) || distance(a, b) <= maxOf(2, a.length / 2)
    }

    /**
     * Autocorreção só troca o quase certo (o corretor às vezes responde em outro idioma): acento ou caixa
     * ("voce" → "você") ou duas vizinhas trocadas ("qeu" → "que"). Nunca nome próprio digitado em minúscula.
     */
    private fun safeToAutocorrect(typed: String, s: String): Boolean {
        if (typed.length < 2) return false
        if (typed.first().isLowerCase() && s.first().isUpperCase()) return false
        val a = fold(typed)
        val b = fold(s)
        if (a == b) return true
        return a.length >= 3 && a.length == b.length && distance(a, b) == 1 && a.toList().sorted() == b.toList().sorted()
    }

    /** Minúsculas e sem acento: "Você" → "voce". */
    private fun fold(s: String): String =
        Normalizer.normalize(s.lowercase(locale ?: Locale.getDefault()), Normalizer.Form.NFD).replace(MARKS, "")

    /** Damerau–Levenshtein (com transposição de vizinhas, o erro de digitação mais comum). */
    private fun distance(a: String, b: String): Int {
        val d = Array(a.length + 1) { IntArray(b.length + 1) }
        for (i in 0..a.length) d[i][0] = i
        for (j in 0..b.length) d[0][j] = j
        for (i in 1..a.length) for (j in 1..b.length) {
            val cost = if (a[i - 1] == b[j - 1]) 0 else 1
            d[i][j] = minOf(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost)
            if (i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1]) d[i][j] = minOf(d[i][j], d[i - 2][j - 2] + 1)
        }
        return d[a.length][b.length]
    }

    private companion object {
        const val MAX = 5
        /** Marcador interno da consulta-canário (nunca é uma palavra digitada: tem espaço). */
        const val CANARY_KEY = " canary "
        const val NONSENSE_KEY = " nonsense "
        const val REPROBE_MS = 3_000L
        const val MAX_PROBES = 10
        /** Não existe em idioma nenhum: um corretor que a aceita aceita tudo. */
        const val NONSENSE = "xqzvbnkw"
        /** Palavra comum de cada idioma do teclado; ausente do dicionário = idioma não suportado. */
        val CANARY = mapOf("pt" to "você", "en" to "the")
        val MARKS = Regex("\\p{Mn}+")
    }
}
