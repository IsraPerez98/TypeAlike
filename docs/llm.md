# Model runner

What `typealike_llm` does: it runs GGUF models with llama.cpp (`llama-cpp-2`) on one worker thread (`ModelRunner`), on the backend that each load names (CPU, Vulkan, CUDA, Metal). The build needs CMake and libclang.

## Backends and threads

- macOS always loads on Metal.
- On the CPU, a prompt runs one thread on each performance core of the `CpuCores` that the bin gives (at least 4 threads when the CPU has 4 cores), and each added token runs on at most 4 threads.
- Without `CpuCores` (macOS, tests, eval), and on the GPU backends, llama.cpp keeps its threads.
- When llama.cpp starts, it logs each device that llama.cpp found, because a release build of llama.cpp loads its GPU backends without a log.

## GPU memory

- The runner refuses a GPU load when the whole model does not fit in free GPU memory with the context of the backend (8192 tokens).
- With a drafter, the check also keeps 64 MiB free for the drafter (`DRAFTER_MEMORY`). When the model fits only without the drafter, it loads without the drafter.
- A read of GPU memory gives the free memory of the device that a load on that backend uses (the Metal device on macOS), and keeps a loaded model.

## Warm-up

- On Vulkan, CUDA and Metal, each load warms up the backend before it answers (`WarmUp`): it makes the context, decodes a prompt of one batch and two added tokens (with a drafter, also a guess and its check), and then forgets them.
- Thus the first completion after a load does not prepare the kernels and the memory of the GPU.
- A request that waits stops the warm-up.
- The CPU does no warm-up, because it costs more CPU time than it saves.

## Metal residency sets

- The residency sets of Metal are off (`disable_metal_residency_sets`, at the start of `main` of the bin, `prompt_eval` and `first_key`).
- With them, the whole model stays wired in memory while it is loaded, and a thread of llama.cpp wakes every 5 ms while the process runs.
- Without them, the first completion after a pause takes about 65 ms more on an M5, and the next keys take the same time or less.

## Lazy reads

- Gemma 4 E2B and E4B have a table of per-layer embeddings (1872 MiB in E4B). llama.cpp keeps it in CPU memory on each backend, mapped from the model file, and each token needs only one row of it.
- A load on the CPU, Metal or a discrete GPU reads only the rows that a batch needs (`LlamaLazyMode::On`). The default of llama.cpp does this only for a table above 4 GiB, and otherwise reads the whole table into memory at load.
- llama.cpp marks only this table for lazy reads, so the setting does not change a Qwen3.5 model.
- On Metal (M5), the most memory of a process (`/usr/bin/time -l`) goes from 4.7 GB to 3.1 GB with E4B, and from 3.0 GB to 1.7 GB with E2B. On the CPU, it goes from 7.3 GB to 5.7 GB with E4B. The suggestions and the times of the keys stay the same. Vulkan and CUDA are not measured.
- The footprint in Activity Monitor (about 470 MB with E4B on Metal) does not change, because it does not count the pages of a mapped file. The gain is the file cache that the table no longer fills.
- When the model file is not in the file cache, a load with its warm-up takes about the same time (1.7 s with E4B). A request that comes before the end of the load waits about 0.4 s more for its rows, but the load takes 0.4 s less.
- An integrated GPU on Vulkan or CUDA reads the whole table at load (`LlamaLazyMode::Off`). It loads without mmap, and lazy reads made the prompts of another model 2 times slower on such a GPU (llama.cpp issue 28160).
- `prompt_eval` also loads the model of its log probabilities with lazy reads, so that `/usr/bin/time -l` shows the memory of the app.

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
- `SentenceEnd::Stop` is the switch "Stop at the end of a sentence" of the General pane, on by default, just below the suggestion length. While it is off, a warning below it (`NoteTone::Warning`: a warning sign, yellow text on macOS) tells that suggestions are often wrong after the first sentence. The text after the first sentence is mostly wrong: on the tuning set (Gemma 4 E4B), the stop gave 176 right and 283 wrong words, against 180 and 488 without it.
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

- The context holds 4096 tokens on the CPU and 8192 tokens on Vulkan, CUDA and Metal (`Backend::context_tokens`), or fewer when the model learned fewer (`n_ctx_train`).
- 8192 tokens hold the longest preface (about 2150 estimated tokens), 9000 characters of text before the caret and the longest suggestion. They also hold 9000 Chinese characters, because Gemma has about 1.7 Chinese characters in each token.
- Each 2048 tokens of context take about 118 MB for Gemma 4 E4B. Thus the context of a graphics card takes about 240 MB more than 4096 tokens (not measured).
- The runner keeps the last prompt in the llama.cpp cache, so the next prompt decodes only the tokens that it adds.
- On Vulkan, CUDA and Metal, after 1 minute without a request (`CONTEXT_IDLE_TIMEOUT`), it frees the context: the cache of the last prompt and the buffers of the backend (for Gemma 4 E4B on Metal, the footprint goes from about 420 MB to 170 MB).
- The next request makes the context again and decodes its whole prompt.
- The CPU keeps its context, because that decode takes seconds on a slow CPU.
- It decodes the new tokens of a prompt in batches of at most 256 tokens on Vulkan, CUDA and Metal, and 128 tokens on the CPU.
- A newer request stops the decode between two batches.
- For a recurrent model (Qwen3.5), it also saves the state before each of the last words, so a backspace across a word does not decode the full prompt again.

## Drafter

- A drafter is a small model that guesses the next token of its model. Gemma 4 E2B and E4B have one: a second file of their repository (`ModelFile::drafter`, `mtp-gemma-4-E2B-it.gguf` and `mtp-gemma-4-E4B-it.gguf`, about 60 MB).
- On Vulkan, CUDA and Metal, when the folder of the model holds its drafter, the runner loads it on the same device (`DrafterUse`). It loads the drafter on a second thread while the model loads, because the drafter alone takes about 0.25 s to load.
- Before each decode of an added token, the drafter guesses the token after it from the state of the model after the token before. The decode then reads the added token and the guess together.
- When the model chooses the guess as the next token, that token needs no decode of its own. Thus a completion takes fewer decodes.
- The model still chooses each token from its own logits, with the healing, the bias, the cut of the tail and each stop.
- A decode of two tokens gives logits that differ in the last digits from a decode of one token. Thus a near tie can end a suggestion at another place: in 1 of 322 cases of the tuning set on Metal, a suggestion got a comma more.
- The first added token of a completion decodes without a guess, because the drafter needs the state of the model after the token before.
- The completion runs in sequence 0 and the prompt stays in sequence 1, because the drafter of llama.cpp reads only sequence 0.
- The drafter of Gemma reads the memory of the context of the model, so it has no memory of its own. Its context decodes 2 tokens at most (`DRAFTER_BATCH_TOKENS`), because the outputs of a batch of 256 tokens take 518 MiB of GPU memory on Metal.
- With a drafter, llama.cpp also keeps the last state of each token of a prompt decode.
- When the drafter is missing, does not fit the model or fails to start, the model runs without it, and the log tells why.
- With `TYPEALIKE_LOG=typealike_llm=debug`, the log gives the guesses and the right guesses of each completion.

### Measured gain

- On an idle M5 with Metal (tuning set, 2 runs on each side, with the sentence stop), the drafter makes the time of a suggestion after the prompt decode (the warm time of `prompt_eval`) 25 percent shorter for Gemma 4 E4B (144 to 108 ms) and 10 percent shorter for E2B (67 to 60 ms). The time on the first key goes from 251 to 230 ms (E4B) and from 144 to 128 ms (E2B). All 161 suggestions stay the same for each model, and the model chooses the guess about 52 percent of the time.
- On the CPU of an M5, Gemma 4 E2B with its drafter was not faster: the warm time of 20 cases went from 114 to 116 ms, with the short suggestions that the CPU allows. Thus the CPU loads no drafter.
- Vulkan and CUDA were not measured. A GPU decodes two tokens in about the time of one, as on Metal, so each right guess saves most of a decode.
- The drafter adds about 60 MiB of GPU memory. With Gemma 4 E4B on an M5, it adds about 80 ms to a load and to its warm-up (`first_key`), the first completion after a load is about 40 ms shorter, and the next key about 40 ms shorter (196 to 157 ms).

## Text of a completion

- The runner sends the text of a completion while it grows.
- That text joins the text of each added token (`token_to_piece`) and uses no detokenize, because the detokenize of llama.cpp removes the space before `? ! . ,` for Gemma 4 (`clean_spaces`). Thus a French "Merci !" keeps its space.
- With the "Show words early" setting, the engine shows its complete words at once.

## Shutdown

- A drop of `LlamaCpp` waits until the worker frees the model, because on Metal llama.cpp stops the process with an assert when the process exits with a loaded model (macOS quits through `exit`).
