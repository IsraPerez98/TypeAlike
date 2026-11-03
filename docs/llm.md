# Model runner

What `typealike_llm` does: it runs GGUF models with llama.cpp (`llama-cpp-2`) on one worker thread (`ModelRunner`), on the backend that each load names (CPU, Vulkan, CUDA, Metal). The build needs CMake and libclang.

## Backends and threads

- macOS always loads on Metal.
- On the CPU, a prompt runs one thread on each performance core of the `CpuCores` that the bin gives (at least 4 threads when the CPU has 4 cores), and each added token runs on at most 4 threads.
- Without `CpuCores` (macOS, tests, eval), and on the GPU backends, llama.cpp keeps its threads.
- When llama.cpp starts, it logs each device that llama.cpp found, because a release build of llama.cpp loads its GPU backends without a log.

## GPU memory

- The runner refuses a GPU load when the whole model does not fit in free GPU memory.
- A read of GPU memory gives the free memory of the device that a load on that backend uses (the Metal device on macOS), and keeps a loaded model.

## Warm-up

- On Vulkan, CUDA and Metal, each load warms up the backend before it answers (`WarmUp`): it makes the context, decodes a prompt of one batch and one added token, and then forgets them.
- Thus the first completion after a load does not prepare the kernels and the memory of the GPU.
- A request that waits stops the warm-up.
- The CPU does no warm-up, because it costs more CPU time than it saves.

## Metal residency sets

- The residency sets of Metal are off (`disable_metal_residency_sets`, at the start of `main` of the bin, `prompt_eval` and `first_key`).
- With them, the whole model stays wired in memory while it is loaded, and a thread of llama.cpp wakes every 5 ms while the process runs.
- Without them, the first completion after a pause takes about 65 ms more on an M5, and the next keys take the same time or less.

## Token healing and word probabilities

- The runner heals the last word of each prompt (token healing), but never takes back a line break.
- It adds the next-word bias only to the first token that it adds.
- It finds the tokens that agree with the healed text in a sorted table of the token texts, which it makes at each load.
- It gives the log probability of each added word (at a healing step, the share of the token among the tokens that agree).

## Typed letters and typo fixes

- It gives the log probability of the typed letters of the last word: the higher of the healing path and the tokens that cover the letters at the first healing step (`TypedLetters`), because the healing path can go through a lone space.
- It adds no text when the letters are less likely than the minimum of the request.
- When the request asks for fixes, it looks among the 64 most likely tokens at the start of the word for one that fixes the letters with one edit (a swap of two letters, a changed, an extra or a missing letter: `correction`).
- The fix keeps at least 3 letters and is not a typo itself.
- Then the completion starts again from the fixed letters with that token, and sends no parts.

## Stops

### Cut of the tail

- After the first word, the completion stops when the added text gets less likely than the cut of the request, and leaves out the word of that token.

### End of a sentence

- When the request asks for it (`SentenceEnd::Stop`), it stops before a new word after the end of a sentence: a period, "!", "?", "…" or the CJK signs 。！？, also before closing quotes and brackets.
- `SentenceEnd::Stop` is the switch "Stop at the end of a sentence" of the General pane, on by default, just below the suggestion length. While it is off, a warning below it (`NoteTone::Warning`: a warning sign, yellow text on macOS) tells that suggestions are often wrong after the first sentence.
- Without that stop, the cut of the tail applies to each sentence on its own, and after the end of a sentence that it added, it takes the most likely token that keeps the text on its line (among the 64 most likely: not a line break, the end of the text or markup), because the model often ends the message there.
- A period ends a sentence only after a word of at least 2 letters, because one letter is often an initial or the end of "e.g." or "a.m.".
- The word must also not be one of the English and Spanish `ABBREVIATIONS`, such as "Mrs.", "Sra." or "pág.".

### Token limit

- At the token limit, it leaves out the last word, unless that word ends a sentence or it is the only word.

### End of a phrase

- Without text after the caret on its line, a stop of the cut of the tail or of the token limit after a word that ends no phrase waits: the completion goes on without the cut for at most 40 percent of the token limit (`PHRASE_END_TOKENS_PERCENT`, 4 tokens at medium), and stops before the first new word after the end of a phrase.
- A word ends a phrase when the text ends with a sign that ends a phrase (`PHRASE_ENDS`: a period, a comma, `; : ! ? …`, the CJK signs and the closing quotes and brackets), or when the model finds such a sign, a line break or the end of the text next with a probability of at least 10 percent (`MIN_PHRASE_END`).
- After a word that needs a next word, such as "the", "to" or "en el", the model almost never does.
- When no end of a phrase comes, the completion goes back to the last end of a phrase, else to the place of the stop.
- While the completion runs, its parts end at the last end of a phrase, or at the place where a waiting stop goes back to, so that the shown text never gets shorter.

## Text after the caret

- With text after the caret on its line (`line_after` of the request), each step also reads how likely the model finds the start of that line next (the tokens that agree with it, without the tokens of only whitespace, because Gemma writes a lone space before digits).
- The added text ends where the model finds the added text and then that line most likely, and the completion stops when the model starts to write that line.
- The parts show only up to that end.
- Before that line, a token that opens a placeholder (an opening square bracket before a letter or a digit) also stops the completion, because the ghost text ends before a placeholder.

## Cache and context

- The runner keeps the last prompt in the llama.cpp cache, so the next prompt decodes only the tokens that it adds.
- On Vulkan, CUDA and Metal, after 1 minute without a request (`CONTEXT_IDLE_TIMEOUT`), it frees the context: the cache of the last prompt and the buffers of the backend (for Gemma 4 E4B on Metal, the footprint goes from about 420 MB to 170 MB).
- The next request makes the context again and decodes its whole prompt.
- The CPU keeps its context, because that decode takes seconds on a slow CPU.
- It decodes the new tokens of a prompt in batches of at most 256 tokens on Vulkan, CUDA and Metal, and 128 tokens on the CPU.
- A newer request stops the decode between two batches.
- For a recurrent model (Qwen3.5), it also saves the state before each of the last words, so a backspace across a word does not decode the full prompt again.

## Text of a completion

- The runner sends the text of a completion while it grows.
- That text joins the text of each added token (`token_to_piece`) and uses no detokenize, because the detokenize of llama.cpp removes the space before `? ! . ,` for Gemma 4 (`clean_spaces`). Thus a French "Merci !" keeps its space.
- With the "Show words early" setting, the engine shows its complete words at once.

## Shutdown

- A drop of `LlamaCpp` waits until the worker frees the model, because on Metal llama.cpp stops the process with an assert when the process exits with a loaded model (macOS quits through `exit`).
