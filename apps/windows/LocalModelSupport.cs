using System;
using System.Collections.Generic;
using System.IO;

namespace Assemblywright.Windows
{
    internal static class DemoJobSettings
    {
        internal static JobDocument CreateSnapshot(GuiSettings settings, string repository, string validationProgram)
        {
            string settingsError = GuiSettingsStore.Validate(settings);
            if (settingsError != null) throw new InvalidDataException(settingsError);

            JobDocument job = new JobDocument();
            job.repository = Path.GetFullPath(repository);
            job.specification = "Prepare the packaged Assemblywright demo repository.";
            job.allowed_files = new List<string> { "src/slugify.py", "src/cli.py" };
            job.validation.program = Path.GetFullPath(validationProgram);
            job.validation.args = new List<string> { "-Demo" };
            job.validation.timeout_seconds = settings.timeout_seconds;
            job.endpoint = settings.endpoint;
            job.model = settings.model;
            job.reasoning_effort = settings.reasoning_effort;
            job.orchestrator_provider = settings.orchestrator_provider;
            job.orchestrator_model = settings.orchestrator_model;
            job.orchestrator_reasoning_effort = settings.orchestrator_reasoning_effort;
            job.reviewer_provider = settings.reviewer_provider;
            job.reviewer_model = settings.reviewer_model;
            job.reviewer_reasoning_effort = settings.reviewer_reasoning_effort;
            job.max_attempts = settings.default_attempts;

            string jobError = JobValidator.ValidateForForm(job, true);
            if (jobError != null) throw new InvalidDataException(jobError);
            return job;
        }
    }
}
