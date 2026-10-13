using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Assemblywright.Windows
{
    internal sealed class ChatReply
    {
        internal string Text;
        internal string RequestId;
    }

    internal static class LocalChatClient
    {
        internal const int MaximumResponseBytes = 2 * 1024 * 1024;
        internal const int MaximumRequestBytes = 2 * 1024 * 1024;
        internal const int TimeoutMilliseconds = 120000;

        internal static async Task<ChatReply> SendAsync(
            string endpoint,
            string model,
            IList<ChatHistoryMessage> history,
            string currentSpecification,
            string ownerText,
            CancellationToken cancellationToken)
        {
            Uri uri = LoopbackEndpoint.ChatCompletions(endpoint);
            if (string.IsNullOrWhiteSpace(model)) throw new InvalidDataException("Enter a local model name.");
            if (string.IsNullOrWhiteSpace(ownerText)) throw new InvalidDataException("Enter a message for the local AI.");
            if (ownerText.Length > 64 * 1024) throw new InvalidDataException("Chat input must be at most 65,536 characters.");

            Dictionary<string, object> requestBody = BuildRequest(model, history, currentSpecification, ownerText);
            byte[] bytes = new UTF8Encoding(false, true).GetBytes(Serializer.New().Serialize(requestBody));
            if (bytes.Length > MaximumRequestBytes) throw new InvalidDataException("The bounded local chat request exceeds 2 MiB.");

            HttpWebRequest request = (HttpWebRequest)WebRequest.Create(uri);
            request.Method = "POST";
            request.ContentType = "application/json; charset=utf-8";
            request.Accept = "application/json";
            request.AllowAutoRedirect = false;
            request.Proxy = null;
            request.KeepAlive = false;
            request.Timeout = TimeoutMilliseconds;
            request.ReadWriteTimeout = TimeoutMilliseconds;
            request.ContentLength = bytes.Length;

            using (CancellationTokenSource deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
            {
                deadline.CancelAfter(TimeoutMilliseconds);
                CancellationToken effectiveCancellation = deadline.Token;
                using (effectiveCancellation.Register(delegate { try { request.Abort(); } catch { } }))
                {
                    try
                    {
                        using (Stream outgoing = await Task.Factory.FromAsync<Stream>(request.BeginGetRequestStream, request.EndGetRequestStream, null))
                        {
                            await outgoing.WriteAsync(bytes, 0, bytes.Length, effectiveCancellation);
                        }
                        using (HttpWebResponse response = (HttpWebResponse)await Task.Factory.FromAsync<WebResponse>(request.BeginGetResponse, request.EndGetResponse, null))
                        {
                            if (response.StatusCode != HttpStatusCode.OK)
                                throw new InvalidDataException("The local AI returned HTTP " + (int)response.StatusCode + ".");
                            if (response.ContentLength > MaximumResponseBytes)
                                throw new InvalidDataException("The local AI response exceeds 2 MiB.");
                            byte[] body = await ReadBoundedAsync(response.GetResponseStream(), effectiveCancellation);
                            return ParseResponse(new UTF8Encoding(false, true).GetString(body));
                        }
                    }
                    catch (WebException exception)
                    {
                        if (effectiveCancellation.IsCancellationRequested)
                        {
                            if (cancellationToken.IsCancellationRequested) throw new OperationCanceledException(cancellationToken);
                            throw new TimeoutException("The local AI request exceeded 120 seconds.");
                        }
                        HttpWebResponse response = exception.Response as HttpWebResponse;
                        if (response != null) throw new InvalidDataException("The local AI returned HTTP " + (int)response.StatusCode + ".");
                        throw new InvalidOperationException("The local AI request failed: " + exception.Message, exception);
                    }
                    catch (OperationCanceledException)
                    {
                        if (cancellationToken.IsCancellationRequested) throw;
                        throw new TimeoutException("The local AI request exceeded 120 seconds.");
                    }
                }
            }
        }

        internal static Dictionary<string, object> BuildRequest(
            string model,
            IList<ChatHistoryMessage> history,
            string currentSpecification,
            string ownerText)
        {
            List<object> messages = new List<object>();
            messages.Add(Message("system",
                "You help the owner turn an idea into a precise Assemblywright feature specification. " +
                "You can propose text only. You cannot run tools, read files, change repositories, enqueue work, publish, or grant approval."));
            string specification = currentSpecification ?? string.Empty;
            if (specification.Length > 32 * 1024) specification = specification.Substring(0, 32 * 1024);
            if (!string.IsNullOrWhiteSpace(specification))
                messages.Add(Message("system", "Current feature specification supplied by the owner:\n" + specification));
            if (history != null)
            {
                int start = Math.Max(0, history.Count - ChatHistoryStore.MaximumMessages);
                for (int index = start; index < history.Count; index++)
                {
                    ChatHistoryMessage item = history[index];
                    if (item == null || item.role == "error" || string.IsNullOrEmpty(item.text)) continue;
                    messages.Add(Message(item.role == "owner" ? "user" : "assistant", item.text));
                }
            }
            messages.Add(Message("user", ownerText));

            Dictionary<string, object> body = new Dictionary<string, object>();
            body["model"] = model;
            body["messages"] = messages;
            body["max_tokens"] = 2048;
            body["temperature"] = 0.1;
            body["stream"] = false;
            Dictionary<string, object> template = new Dictionary<string, object>();
            template["enable_thinking"] = false;
            body["chat_template_kwargs"] = template;
            return body;
        }

        internal static ChatReply ParseResponse(string json)
        {
            object rootValue = Serializer.New().DeserializeObject(json);
            Dictionary<string, object> root = rootValue as Dictionary<string, object>;
            if (root == null) throw new InvalidDataException("The local AI returned invalid JSON.");
            object choicesValue;
            if (!root.TryGetValue("choices", out choicesValue)) throw new InvalidDataException("The local AI response has no choices.");
            object[] choices = choicesValue as object[];
            if (choices == null || choices.Length < 1) throw new InvalidDataException("The local AI response has no choices.");
            Dictionary<string, object> choice = choices[0] as Dictionary<string, object>;
            object messageValue;
            if (choice == null || !choice.TryGetValue("message", out messageValue)) throw new InvalidDataException("The local AI response has no message.");
            Dictionary<string, object> message = messageValue as Dictionary<string, object>;
            object contentValue;
            string content = message != null && message.TryGetValue("content", out contentValue) ? contentValue as string : null;
            if (string.IsNullOrWhiteSpace(content)) throw new InvalidDataException("The local AI returned an empty answer.");
            if (content.Length > ChatHistoryStore.MaximumTextCharacters) throw new InvalidDataException("The local AI answer exceeds 131,072 characters.");
            object idValue;
            ChatReply reply = new ChatReply();
            reply.Text = content.Trim();
            reply.RequestId = root.TryGetValue("id", out idValue) && idValue is string ? (string)idValue : string.Empty;
            return reply;
        }

        private static Dictionary<string, object> Message(string role, string content)
        {
            Dictionary<string, object> message = new Dictionary<string, object>();
            message["role"] = role;
            message["content"] = content;
            return message;
        }

        private static async Task<byte[]> ReadBoundedAsync(Stream stream, CancellationToken cancellationToken)
        {
            using (MemoryStream output = new MemoryStream())
            {
                byte[] buffer = new byte[16 * 1024];
                while (true)
                {
                    int count = await stream.ReadAsync(buffer, 0, buffer.Length, cancellationToken);
                    if (count == 0) break;
                    if (output.Length + count > MaximumResponseBytes)
                        throw new InvalidDataException("The local AI response exceeds 2 MiB.");
                    output.Write(buffer, 0, count);
                }
                return output.ToArray();
            }
        }
    }

    internal static class ChatGptChatClient
    {
        internal const int MaximumBytes = 2 * 1024 * 1024;
        internal const int TimeoutMilliseconds = 300000;

        internal static async Task<ChatReply> SendAsync(string runnerPath, string stateDirectory, string model, string reasoningEffort,
            IList<ChatHistoryMessage> history, string currentSpecification, string ownerText, CancellationToken cancellationToken)
        {
            if (string.IsNullOrWhiteSpace(model) || model.Length > 200) throw new InvalidDataException("Choose a ChatGPT orchestrator model in Settings.");
            if (!ReasoningEffort.IsValid(reasoningEffort) || string.IsNullOrWhiteSpace(reasoningEffort)) throw new InvalidDataException("Choose a ChatGPT orchestrator reasoning level in Settings.");
            if (string.IsNullOrWhiteSpace(ownerText)) throw new InvalidDataException("Enter a message for ChatGPT.");
            if (ownerText.Length > 64 * 1024) throw new InvalidDataException("Chat input must be at most 65,536 characters.");
            string prompt = BuildPlanningPrompt(history, currentSpecification, ownerText, model);
            Dictionary<string, object> request = new Dictionary<string, object>();
            request["model"] = model; request["reasoning_effort"] = reasoningEffort; request["prompt"] = prompt;
            string requestJson = Serializer.New().Serialize(request);
            if (Encoding.UTF8.GetByteCount(requestJson) > MaximumBytes) throw new InvalidDataException("The bounded ChatGPT planning request exceeds 2 MiB.");
            string requestDirectory = Path.Combine(stateDirectory, "chatgpt", "gui-requests");
            Directory.CreateDirectory(requestDirectory);
            string requestPath = Path.Combine(requestDirectory, Guid.NewGuid().ToString("N") + ".json");
            JobJson.WriteUtf8(requestPath, requestJson);
            BoundedCapture output = new BoundedCapture(MaximumBytes);
            BoundedCapture error = new BoundedCapture(256 * 1024);
            Process process = null;
            try
            {
                process = ProcessRunner.StartStreaming(runnerPath, new string[] { "chatgpt-chat", "--state-dir", stateDirectory, "--request", requestPath }, output.Append, error.Append);
                using (CancellationTokenSource deadline = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
                {
                    deadline.CancelAfter(TimeoutMilliseconds);
                    using (deadline.Token.Register(delegate
                    {
                        try { if (!process.HasExited) process.Kill(); } catch { }
                        try { process.WaitForExit(5000); } catch { }
                    }))
                    {
                        await Task.Run(delegate { process.WaitForExit(); });
                    }
                    if (deadline.IsCancellationRequested)
                    {
                        if (cancellationToken.IsCancellationRequested) throw new OperationCanceledException(cancellationToken);
                        throw new TimeoutException("The ChatGPT planning request exceeded 300 seconds.");
                    }
                }
                if (output.Overflowed || error.Overflowed) throw new InvalidDataException("The ChatGPT planning response exceeded its bounded output limit.");
                if (process.ExitCode != 0) throw new InvalidOperationException(string.IsNullOrWhiteSpace(error.Text) ? "The ChatGPT planning request failed." : error.Text.Trim());
                return ParseResponse(output.Text, model, reasoningEffort);
            }
            finally
            {
                if (process != null) process.Dispose();
                try { if (File.Exists(requestPath)) File.Delete(requestPath); } catch { }
            }
        }

        internal static string BuildPlanningPrompt(IList<ChatHistoryMessage> history, string currentSpecification, string ownerText, string selectedModel)
        {
            StringBuilder prompt = new StringBuilder();
            prompt.AppendLine("You help the owner turn an idea into a precise Assemblywright feature specification.");
            prompt.AppendLine("Respond with planning text only. Do not write implementation code. You have no tools, repository access, queue authority, publication authority, or approval authority.");
            string specification = currentSpecification ?? string.Empty;
            if (specification.Length > 32 * 1024) specification = specification.Substring(0, 32 * 1024);
            if (!string.IsNullOrWhiteSpace(specification)) prompt.AppendLine().AppendLine("Current owner-supplied feature specification:").AppendLine(specification);
            if (history != null)
            {
                int start = Math.Max(0, history.Count - ChatHistoryStore.MaximumMessages);
                for (int index = start; index < history.Count; index++)
                {
                    ChatHistoryMessage item = history[index];
                    string provider = item == null || string.IsNullOrWhiteSpace(item.provider) ? RoleProvider.Local : item.provider;
                    if (item == null || item.role == "error" || string.IsNullOrEmpty(item.text) || provider != RoleProvider.ChatGpt || item.model != selectedModel) continue;
                    prompt.AppendLine().Append(item.role == "owner" ? "Owner: " : "Assistant: ").AppendLine(item.text);
                }
            }
            prompt.AppendLine().Append("Owner: ").AppendLine(ownerText).Append("Assistant:");
            return prompt.ToString();
        }

        internal static ChatReply ParseResponse(string json, string requestedModel, string requestedEffort)
        {
            object parsed = Serializer.New().DeserializeObject(json);
            Dictionary<string, object> root = parsed as Dictionary<string, object>;
            ChatGptModelCatalog.RequireKeys(root, new string[] { "status", "text", "provider", "model", "reasoning_effort" }, "The ChatGPT planning response is invalid.");
            string text = Convert.ToString(root["text"]);
            if (Convert.ToString(root["status"]) != "completed" || Convert.ToString(root["provider"]) != RoleProvider.ChatGpt
                || Convert.ToString(root["model"]) != requestedModel || Convert.ToString(root["reasoning_effort"]) != requestedEffort
                || string.IsNullOrWhiteSpace(text) || text.Length > ChatHistoryStore.MaximumTextCharacters)
                throw new InvalidDataException("The ChatGPT planning response did not match the selected model and reasoning level.");
            ChatReply reply = new ChatReply(); reply.Text = text.Trim(); reply.RequestId = string.Empty; return reply;
        }
    }
}
