import { Component, Input, ChangeDetectionStrategy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { cn } from '../utils';

@Component({
  selector: 'app-card',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="cn('rounded-lg border bg-card text-card-foreground shadow-sm', className)">
      <ng-content></ng-content>
    </div>
  `,
})
export class CardComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-card-header',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="cn('flex flex-col space-y-1.5 p-6', className)">
      <ng-content></ng-content>
    </div>
  `,
})
export class CardHeaderComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-card-title',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <h3 [class]="cn('text-2xl font-semibold leading-none tracking-tight', className)">
      <ng-content></ng-content>
    </h3>
  `,
})
export class CardTitleComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-card-description',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <p [class]="cn('text-sm text-muted-foreground', className)">
      <ng-content></ng-content>
    </p>
  `,
})
export class CardDescriptionComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-card-content',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="cn('p-6 pt-0', className)">
      <ng-content></ng-content>
    </div>
  `,
})
export class CardContentComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-card-footer',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="cn('flex items-center p-6 pt-0', className)">
      <ng-content></ng-content>
    </div>
  `,
})
export class CardFooterComponent {
  @Input() className = '';
  cn = cn;
}