#include <sys/wait.h>
#include <sys/types.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <getopt.h>
#include <signal.h>
#include <unistd.h>
#include <list.h>

#define VERSION "0.4"

#define RL_BUFSIZE 1024
#define TOK_BUFSIZE 64
#define TOK_DELIM " \t\r\n\a"

struct bg_proc
{
    pid_t pid;
    int status; // Running, Done, Done(retval), Stopped, Stopped(SIGTSTP), Stopped(SIGSTOP), Stopped(SIGTTIN), Stopped(SIGTTOU)
    int retval;
    char* name;
};

int sh_cd( char** args );
int sh_help( char** args );
int sh_jobs( char** args );
int sh_bg( char** args );
int sh_fg( char** args );
int sh_exit( char** args );

// Builtin function names
char* builtin_str[] =
{
    "cd",
    "help",
    "jobs",
    "bg",
    "fg",
    "exit",
};

// Builtin function descriptions
char* builtin_desc[] =
{
    "[dir] - Change current working directory",
    "- Display this help prompt",
    "- List all jobs",
    "- Places the current or specified job in the background",
    "- Brings the current or specified job into the foreground",
    "- Exit shell",
};

// Builtin function pointers
int (*builtin_func[])( char** ) =
{
    &sh_cd,
    &sh_help,
    &sh_jobs,
    &sh_bg,
    &sh_fg,
    &sh_exit,
};

int exit_sh = 0;
pid_t sh_pgid;
list_t* background;

void sh_loop( void ); // Main shell loop: prompt, readline, splitline, execute
char* sh_readline( void ); // Read line of input from stdio, replace \n with \0
char** sh_splitline( char* line ); // Split line into different strings, return array of arguments
int sh_execute( char** args ); // Check if command is builtin, if not run sh_launch
int sh_launch( char** args ); // Fork and execvp, wait for child to exit
int sh_num_builtins( void ); // Number of builtin functions
void bg_proc_status( char* bg_status, int status, int retval ); // Return description of bg_proc->status
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    // TODO: setup

    int c;

    if( argc != 1 )
    {
        while( 1 )
        {
            static struct option long_options[] =
            {
                {"version", no_argument, 0, 'v'},
                {"help", no_argument, 0, 'h'},
                {0, 0, 0, 0}
            };
 
            int option_index = 0;
 
            c = getopt_long(argc, argv, "", long_options, &option_index);
 
            if( c == -1 )
            {
                break;
            }
 
            switch( c )
            {
                case 'v':
                    show_version();
                    __builtin_unreachable();
                case '?':
                    fprintf(stderr, "Try 'sh --help'\n");
                    exit(EXIT_FAILURE);
                case 'h':
                default:
                    show_usage();
                    __builtin_unreachable();
            }
        }
    }

    if( getenv("PATH") == NULL )
    {
        putenv("PATH=/bin");
    }
    setlinebuf(stdout);

    sh_pgid = getpgid(0);

    signal(SIGINT, SIG_IGN);
    signal(SIGTSTP, SIG_IGN);

    background = list_create();

    // Main shell loop
    sh_loop();

    // TODO: cleanup

    foreach(node, background)
    {
        kill(((struct bg_proc*)node->value)->pid, SIGHUP);
    }

    list_destroy(background);

    return EXIT_SUCCESS;
}

// Main shell loop: prompt, readline, splitline, execute
void sh_loop( void )
{
    char* line;
    char** args;
    int status = 0;
    char cwd[1024];

    do{
        getcwd(cwd, 1023);
        printf("\033[1;33m%s", cwd);
        if( status && WIFEXITED(status) )
        {
            printf("\033[0;41m<\u255d%d\033[0m", WEXITSTATUS(status));
        }else if( status && WIFSIGNALED(status) )
        {
            printf("\033[0;41m%s(%d)\033[0m", sys_signame[WTERMSIG(status)], WTERMSIG(status));
        }
        printf("\033[38;5;2m$\033[0m ");
        fflush(stdout);
        line = sh_readline(); // Read line from stdin
        args = sh_splitline(line); // Split line into tokens
        status = sh_execute(args); // Execute tokenized input 

        free(line);
        free(args);
    }while( !exit_sh );
}

// Read line of input from stdio, replace \n with \0
char* sh_readline( void )
{
    int bufsize = RL_BUFSIZE;
    int position = 0;
    char* buffer = malloc(sizeof(char) * bufsize);
    int c;

    if( !buffer )
    {
        fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
        exit(EXIT_FAILURE);
    }

    while( 1 )
    {
        c = getchar();

        if( c == EOF )
        {
            exit(EXIT_SUCCESS);
        }else if( c == '\n' )
        {
            buffer[position] = '\0';
            return buffer;
        }else
        {
            buffer[position] = c;
        }
        position++;

        if( position >= bufsize )
        {
            bufsize += RL_BUFSIZE;
            buffer = realloc(buffer, bufsize);
            if( !buffer )
            {
                fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
                exit(EXIT_FAILURE);
            }
        }
    }
}

// Split line into different strings, return array of arguments
char** sh_splitline( char* line )
{
    int bufsize = TOK_BUFSIZE;
    int position = 0;
    char** tokens = malloc(bufsize * sizeof(char*));
    char* token, **tokens_backup;

    if( !tokens )
    {
        fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
        exit(EXIT_FAILURE);
    }

    token = strtok(line, TOK_DELIM);
    while( token != NULL )
    {
        tokens[position] = token;
        position++;

        if( position >= bufsize )
        {
            bufsize += TOK_BUFSIZE;
            tokens_backup = tokens;
            tokens = realloc(tokens, bufsize * sizeof(char*));
            if( !tokens )
            {
                free(tokens_backup);
                fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
                exit(EXIT_FAILURE);
            }
        }

        token = strtok(NULL, TOK_DELIM);
    }

    tokens[position] = NULL;
    return tokens;
}

// Check if command is builtin, if not run sh_launch
int sh_execute( char** args )
{
    if( args[0] == NULL )
    {
        return 0;
    }

    for( int i = 0; i < sh_num_builtins(); i++ )
    {
        if( strcmp(args[0], builtin_str[i]) == 0 )
        {
            return (*builtin_func[i])(args);
        }
    }

    return sh_launch(args);
}

// Fork and execvp, wait for child to exit
int sh_launch( char** args )
{
    pid_t pid;
    int status = 0;

    pid = fork();
    if( pid == 0 )
    {
        // Child process
        setpgid(0, 0);
        tcsetpgrp(STDIN_FILENO, getpid());
        if( execvp(args[0], args) == -1 )
        {
            printf("\033[0;31msh: Command not found\n"); // Red
        }
        exit(127);
    }else if( pid < 0 )
    {
        // Fork error
        perror("\033[0;31msh"); // Red
    }else
    {
        // Parent process
        do{
            waitpid(pid, &status, WUNTRACED);
            if( WIFSTOPPED(status) )
            {
                struct bg_proc* bg_proc = malloc(sizeof(struct bg_proc));
                bg_proc->pid = pid;
                if( WSTOPSIG(status) == SIGTSTP )
                {
                    bg_proc->status = 4;
                }else if( WSTOPSIG(status) == SIGSTOP )
                {
                    bg_proc->status = 5;
                }else if( WSTOPSIG(status) == SIGTTIN )
                {
                    bg_proc->status = 6;
                }else if( WSTOPSIG(status) == SIGTTOU )
                {
                    bg_proc->status = 7;
                }
                bg_proc->name = strdup(args[0]);
                list_insert(background, bg_proc);
                char bg_status[18];
                bg_proc_status(bg_status, bg_proc->status, bg_proc->retval);
                fprintf(stderr, "[%zd] %d %s %s\n", list_index_of(background, bg_proc) + 1, bg_proc->pid, bg_status, bg_proc->name);
                break;
            }
        }while( !WIFEXITED(status) && !WIFSIGNALED(status) );

        tcsetpgrp(STDIN_FILENO, sh_pgid);
    }

    return status;
}

// Number of builtin functions
int sh_num_builtins( void )
{
    return sizeof(builtin_str) / sizeof(char*);
}

// Change directory
int sh_cd( char** args )
{
    if( args[1] == NULL )
    {
        fprintf(stderr, "\033[0;31msh: expected argument to cd\n"); // Red
    }else
    {
        if( chdir(args[1]) != 0 )
        {
            perror("\033[0;31msh"); // Red
            return 1 << 8;
        }
    }

    return 0;
}

// Display help text
int sh_help( char** args __attribute__((unused)) )
{
    printf("\033[1;36mseraph\033[0m shell (/bin/sh)\n\n");
    printf("Builtin commands:\n");

    for( int i = 0; i < sh_num_builtins(); i++ )
    {
        printf("\t%s %s\n", builtin_str[i], builtin_desc[i]);
    }

    return 0;
}

int sh_jobs( char** args __attribute__((unused)) )
{
    foreach(node, background)
    {
        struct bg_proc* bg_proc = node->value;
        char bg_status[18];
        bg_proc_status(bg_status, bg_proc->status, bg_proc->retval);
        fprintf(stderr, "[%zd] %d %s %s\n", list_index_of(background, bg_proc) + 1, bg_proc->pid, bg_status, bg_proc->name);
    }

    return 0;
}

int sh_bg( char** args __attribute__((unused)) )
{
    return 0;
}

int sh_fg( char** args __attribute__((unused)) )
{
    return 0;
}

// Exit shell
int sh_exit( char** args __attribute__((unused)) )
{
    exit_sh = 1;

    return 0;
}

// Returns description of bg_proc->status
void bg_proc_status( char* bg_status, int status, int retval )
{
    if( status == 0 )
    {
        strcpy(bg_status, "Running");
    }else if( status == 1 )
    {
        strcpy(bg_status, "Done");
    }else if( status == 2 )
    {
        sprintf(bg_status, "Done (%d)", retval);
    }else if( status == 3 )
    {
        strcpy(bg_status, "Stopped");
    }else if( status == 4 )
    {
        strcpy(bg_status, "Stopped (SIGTSTP)");
    }else if( status == 5 )
    {
        strcpy(bg_status, "Stopped (SIGSTOP)");
    }else if( status == 6 )
    {
        strcpy(bg_status, "Stopped (SIGTTIN)");
    }else if( status == 7 )
    {
        strcpy(bg_status, "Stopped (SIGTTOU)");
    }else
    {
        strcpy(bg_status, "ERROR");
    }
}

// Display version text and exit
void show_version( void )
{
    printf("\033[1;36mseraph\033[0m sh %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage sh [OPTION(s)]\n"
           "     --version display version text and exit\n"
           "     --help display this help text and exit\n");

    exit(EXIT_SUCCESS);
}
